// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! Runs a small guest that talks to two emulated MMIO devices.
//!
//! Run with `cargo run -p hvf --example mmio_guest`; the cargo runner of this
//! repository signs the binary with the hypervisor entitlement.
//!
//! The example shows the pieces a VMM built on `hvf` puts together:
//!
//! - guest RAM from `vm-memory`, mapped into the VM with `Vm::map`;
//! - MMIO devices on a `vm-device` [`Bus`], reached through data abort exits
//!   that the VMM decodes and completes with `Vcpu::mmio_write_value`,
//!   `Vcpu::complete_mmio_read` and `Vcpu::advance_pc`;
//! - a hypercall (`hvc`) that ends the guest, after which the VMM reads the
//!   guest's registers.
//!
//! The guest prints a message through a write-only console register, then
//! reads a magic value from an identification device, adds one, and writes
//! the result back.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod guest {
    use std::error::Error;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use hvf::{DataAbort, ExceptionClass, Hvf, MemoryFlags, Reg, SysReg, Vcpu, VcpuExit, VmConfig};
    use vm_device::DeviceMmio;
    use vm_device::bus::{Bus, BusRange, MmioAddress, MmioAddressOffset};
    use vm_memory::{Bytes, GuestAddress, GuestMemoryBackend, GuestMemoryMmap};

    /// Guest physical address of the guest RAM, and of the code in it.
    const RAM_BASE: u64 = 0x4000_0000;
    /// Size of the guest RAM; a multiple of every IPA granule.
    const RAM_SIZE: usize = 0x1_0000;
    /// The console device: writing a byte at offset 0 prints it.
    const CONSOLE_BASE: u64 = 0x0900_0000;
    /// The identification device: offset 0 reads [`MAGIC`], offset 4
    /// records what the guest writes.
    const ID_BASE: u64 = 0x0900_1000;
    /// The value the identification device reads as.
    const MAGIC: u32 = 0x4856_4621;

    /// The guest code, loaded at [`RAM_BASE`]:
    ///
    /// ```text
    /// 0x00: mov   x1, #0x09000000     ; console
    /// 0x04: adr   x2, msg             ; msg = 0x30
    /// 0x08: ldrb  w0, [x2], #1        ; next byte of the message
    /// 0x0c: cbz   w0, 0x18
    /// 0x10: strb  w0, [x1]            ; print it
    /// 0x14: b     0x08
    /// 0x18: add   x3, x1, #0x1000     ; identification device
    /// 0x1c: ldr   w4, [x3]            ; read the magic value
    /// 0x20: add   w4, w4, #1
    /// 0x24: str   w4, [x3, #4]        ; write it back, plus one
    /// 0x28: hvc   #0                  ; done
    /// 0x2c: b     .
    /// 0x30: msg:  .asciz "..."
    /// ```
    const CODE: [u32; 12] = [
        0xd2a1_2001,
        0x1000_0162,
        0x3840_1440,
        0x3400_0060,
        0x3900_0020,
        0x17ff_fffd,
        0x9140_0423,
        0xb940_0064,
        0x1100_0484,
        0xb900_0464,
        0xd400_0002,
        0x1400_0000,
    ];
    /// Offset of the message in the guest image (the `adr` above).
    const MSG_OFFSET: u64 = 0x30;
    /// The message the guest prints.
    const MSG: &[u8] = b"Hello from an HVF guest!\n\0";

    /// A write-only console that forwards bytes to the host's stdout.
    #[derive(Debug, Default)]
    struct Console {
        output: Mutex<Vec<u8>>,
    }

    impl DeviceMmio for Console {
        fn mmio_read(&self, _base: MmioAddress, _offset: MmioAddressOffset, data: &mut [u8]) {
            data.fill(0);
        }

        fn mmio_write(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &[u8]) {
            if offset == 0 {
                self.output.lock().unwrap().extend_from_slice(&data[..1]);
                let mut stdout = std::io::stdout().lock();
                // Printing is best effort.
                let _ = stdout.write_all(&data[..1]);
                let _ = stdout.flush();
            }
        }
    }

    /// A device with a read-only identification register and a scratch
    /// register.
    #[derive(Debug, Default)]
    struct IdDevice {
        scratch: Mutex<Option<u32>>,
    }

    impl DeviceMmio for IdDevice {
        fn mmio_read(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &mut [u8]) {
            let value = match offset {
                0 => MAGIC,
                4 => self.scratch.lock().unwrap().unwrap_or(0),
                _ => 0,
            };
            let len = data.len().min(4);
            data.fill(0);
            data[..len].copy_from_slice(&value.to_le_bytes()[..len]);
        }

        fn mmio_write(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &[u8]) {
            if offset == 4 && data.len() == 4 {
                let value = u32::from_le_bytes(data.try_into().unwrap());
                *self.scratch.lock().unwrap() = Some(value);
            }
        }
    }

    type MmioBus = Bus<MmioAddress, Arc<dyn DeviceMmio + Send + Sync>>;

    /// Emulates one MMIO access and completes the trapped instruction.
    fn handle_mmio(
        vcpu: &mut Vcpu,
        bus: &MmioBus,
        abort: &DataAbort,
    ) -> Result<(), Box<dyn Error>> {
        // Without an instruction syndrome, the VMM would have to decode the
        // instruction itself; this guest only uses simple loads and stores.
        let access = abort
            .syndrome
            .ok_or_else(|| format!("data abort at {:#x} without a syndrome", abort.ipa))?;
        let len = usize::from(access.len);
        let (range, device) = bus
            .check_access(MmioAddress(abort.ipa), len)
            .map_err(|e| format!("MMIO access at {:#x} ({len} bytes): {e}", abort.ipa))?;
        let offset = abort.ipa - range.base().0;
        if abort.write {
            let value = vcpu.mmio_write_value(&access)?;
            device.mmio_write(range.base(), offset, &value.to_le_bytes()[..len]);
            vcpu.advance_pc()?;
        } else {
            let mut bytes = [0u8; 8];
            device.mmio_read(range.base(), offset, &mut bytes[..len]);
            vcpu.complete_mmio_read(&access, u64::from_le_bytes(bytes))?;
        }
        Ok(())
    }

    pub fn run() -> Result<(), Box<dyn Error>> {
        println!(
            "host: page size {:#x}, up to {} vCPUs, {}-bit IPA (default {})",
            Hvf::host_page_size(),
            Hvf::max_vcpu_count()?,
            Hvf::max_ipa_bits()?,
            Hvf::default_ipa_bits()?,
        );

        // The guest RAM is created first so that it outlives the VM.
        let mem = GuestMemoryMmap::<()>::from_ranges(&[(GuestAddress(RAM_BASE), RAM_SIZE)])?;
        let image: Vec<u8> = CODE.iter().flat_map(|w| w.to_le_bytes()).collect();
        mem.write_slice(&image, GuestAddress(RAM_BASE))?;
        mem.write_slice(MSG, GuestAddress(RAM_BASE + MSG_OFFSET))?;

        let vm = match Hvf::create_vm(&VmConfig::default()) {
            Ok(vm) => vm,
            Err(hvf::Error::Hv { err, .. }) if err.is_unavailable() => {
                return Err(format!(
                    "Hypervisor.framework is unavailable ({err}); is the binary signed with \
                     the com.apple.security.hypervisor entitlement?"
                )
                .into());
            }
            Err(e) => return Err(e.into()),
        };
        println!(
            "vm: {} KiB IPA granule, {}-bit IPA",
            vm.ipa_granule() / 1024,
            vm.ipa_bits()
        );
        let host = mem.get_host_address(GuestAddress(RAM_BASE))?;
        // SAFETY: `mem` is dropped after the VM and its vCPU, and the host
        // only accesses it through `vm-memory`'s volatile `Bytes` interface.
        unsafe { vm.map(host, RAM_BASE, RAM_SIZE, MemoryFlags::RWX)? };

        let console = Arc::new(Console::default());
        let id = Arc::new(IdDevice::default());
        let mut bus = MmioBus::new();
        bus.register(
            BusRange::new(MmioAddress(CONSOLE_BASE), 0x1000)?,
            console.clone(),
        )?;
        bus.register(BusRange::new(MmioAddress(ID_BASE), 0x1000)?, id.clone())?;

        // The vCPU belongs to this thread.
        let mut vcpu = vm.create_vcpu(None)?;
        vcpu.set_sys_reg(SysReg::MPIDR_EL1, 1 << 31)?;
        vcpu.set_reg(Reg::PC, RAM_BASE)?;
        // EL1h with all interrupts masked; the MMU stays off.
        vcpu.set_reg(Reg::CPSR, 0x3c5)?;

        println!("guest output:");
        let mut mmio_exits = 0;
        loop {
            match vcpu.run()? {
                VcpuExit::Exception(exception) => match exception.decode() {
                    ExceptionClass::DataAbort(abort) => {
                        mmio_exits += 1;
                        handle_mmio(&mut vcpu, &bus, &abort)?;
                    }
                    // The PC is already past the `hvc`.
                    ExceptionClass::Hvc { imm: 0 } => break,
                    other => {
                        let pc = vcpu.get_reg(Reg::PC)?;
                        return Err(format!("unexpected exception {other:?} at {pc:#x}").into());
                    }
                },
                other => return Err(format!("unexpected exit {other:?}").into()),
            }
        }

        let written = *id.scratch.lock().unwrap();
        let x4 = vcpu.get_reg(Reg::x(4).unwrap())?;
        println!("guest done after {mmio_exits} MMIO exits");
        let written_text = written.map_or_else(|| "nothing".to_string(), |v| format!("{v:#x}"));
        println!("id device: read {MAGIC:#x}, guest wrote back {written_text}, x4 = {x4:#x}");
        let expected = MAGIC + 1;
        if written != Some(expected) || x4 != u64::from(expected) {
            return Err(format!("expected {expected:#x} from the guest").into());
        }
        let printed = console.output.lock().unwrap().clone();
        if printed != MSG[..MSG.len() - 1] {
            return Err(format!("unexpected console output {printed:?}").into());
        }
        println!("guest ran for {} Mach time units", vcpu.exec_time()?);

        // Tear down in order: the vCPU, the guest mapping, then the VM.
        drop(vcpu);
        vm.unmap(RAM_BASE, RAM_SIZE)?;
        drop(vm);
        drop(mem);
        Ok(())
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() {
    if let Err(e) = guest::run() {
        eprintln!("mmio_guest: {e}");
        std::process::exit(1);
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("This example requires macOS on Apple silicon.");
}
