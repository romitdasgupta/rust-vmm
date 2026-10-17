// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! A minimal `/init` for the `boot_linux` example.
//!
//! It is a static arm64 Linux executable without libc: every system call is a
//! raw `svc #0`. It prints `HVF-BOOT-OK` and the `uname(2)` fields on the
//! console, then powers the machine off with
//! `reboot(LINUX_REBOOT_CMD_POWER_OFF)`, which Linux turns into a PSCI
//! `SYSTEM_OFF` call. The VMM then exits with status 0.
//!
//! Build it with `mkinitramfs.sh --init rust`, which also packs it into an
//! initramfs.

#![no_std]
#![no_main]

use core::arch::asm;

// arm64 system call numbers (include/uapi/asm-generic/unistd.h).
const SYS_DUP3: usize = 24;
const SYS_FCNTL: usize = 25;
const SYS_MKDIRAT: usize = 34;
const SYS_MOUNT: usize = 40;
const SYS_OPENAT: usize = 56;
const SYS_CLOSE: usize = 57;
const SYS_WRITE: usize = 64;
const SYS_SYNC: usize = 81;
const SYS_EXIT: usize = 93;
const SYS_REBOOT: usize = 142;
const SYS_UNAME: usize = 160;

const AT_FDCWD: isize = -100;
const O_RDWR: usize = 0o2;
const O_NOCTTY: usize = 0o400;
const F_GETFD: usize = 1;

const LINUX_REBOOT_MAGIC1: usize = 0xfee1_dead;
const LINUX_REBOOT_MAGIC2: usize = 672_274_793;
const LINUX_REBOOT_CMD_POWER_OFF: usize = 0x4321_fedc;

/// Issues system call `nr` with up to five arguments and returns its result
/// (a negative errno on failure).
///
/// # Safety
///
/// The arguments must be valid for system call `nr`: pointers must point to
/// memory of the size and mutability the call expects.
unsafe fn syscall(nr: usize, args: [usize; 5]) -> isize {
    let ret: isize;
    // SAFETY: `svc #0` enters the kernel, which reads the arguments from
    // x0-x4 and the number from x8, and writes the result to x0. It clobbers
    // no other register and does not touch the stack. The caller guarantees
    // that the arguments are valid for the call.
    unsafe {
        asm!(
            "svc #0",
            in("x8") nr,
            inlateout("x0") args[0] => ret,
            in("x1") args[1],
            in("x2") args[2],
            in("x3") args[3],
            in("x4") args[4],
            options(nostack),
        );
    }
    ret
}

/// Writes all of `buf` to file descriptor `fd`, giving up on an error.
fn write(fd: usize, buf: &[u8]) {
    let mut done = 0;
    while done < buf.len() {
        let rest = &buf[done..];
        // SAFETY: `rest` is valid for reads of `rest.len()` bytes.
        let n = unsafe { syscall(SYS_WRITE, [fd, rest.as_ptr() as usize, rest.len(), 0, 0]) };
        if n <= 0 {
            return;
        }
        done += n as usize;
    }
}

fn puts(s: &[u8]) {
    write(1, s);
}

/// Prints a field of `struct utsname`, which is NUL-terminated.
fn put_field(field: &[u8]) {
    let len = field.iter().position(|&b| b == 0).unwrap_or(field.len());
    puts(&field[..len]);
}

fn put_dec(v: isize) {
    let mut buf = [0u8; 21];
    let mut i = buf.len();
    let mut n = v.unsigned_abs();
    loop {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    if v < 0 {
        i -= 1;
        buf[i] = b'-';
    }
    puts(&buf[i..]);
}

/// `struct utsname` of Linux.
#[repr(C)]
struct Utsname {
    sysname: [u8; 65],
    nodename: [u8; 65],
    release: [u8; 65],
    version: [u8; 65],
    machine: [u8; 65],
    domainname: [u8; 65],
}

/// Mounts devtmpfs on `/dev` and, if the kernel could not give init a
/// console (no `/dev/console` in the initramfs), opens `/dev/console` as
/// file descriptors 0, 1 and 2.
fn setup_console() {
    let dev = c"/dev".as_ptr() as usize;
    let devtmpfs = c"devtmpfs".as_ptr() as usize;
    let console = c"/dev/console".as_ptr() as usize;
    // SAFETY: the strings are NUL-terminated and outlive the calls; the
    // other arguments are plain integers. Failures are harmless (for
    // example EEXIST from mkdirat) and ignored.
    unsafe {
        let have_stdout = syscall(SYS_FCNTL, [1, F_GETFD, 0, 0, 0]) >= 0;
        syscall(SYS_MKDIRAT, [AT_FDCWD as usize, dev, 0o755, 0, 0]);
        syscall(SYS_MOUNT, [devtmpfs, dev, devtmpfs, 0, 0]);
        if have_stdout {
            return;
        }
        let fd = syscall(
            SYS_OPENAT,
            [AT_FDCWD as usize, console, O_RDWR | O_NOCTTY, 0, 0],
        );
        if fd < 0 {
            return;
        }
        let fd = fd as usize;
        for target in 0..3 {
            if fd != target {
                syscall(SYS_DUP3, [fd, target, 0, 0, 0]);
            }
        }
        if fd > 2 {
            syscall(SYS_CLOSE, [fd, 0, 0, 0, 0]);
        }
    }
}

/// Exits the process, which as PID 1 makes the kernel panic.
fn exit(status: usize) -> ! {
    // SAFETY: `exit` takes a plain integer and does not return.
    unsafe { syscall(SYS_EXIT, [status, 0, 0, 0, 0]) };
    loop {
        core::hint::spin_loop();
    }
}

/// The entry point. The kernel starts `/init` here with the stack pointer
/// set up and `argc` at the top of the stack, which this program ignores.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    setup_console();

    puts(b"HVF-BOOT-OK: hello from the Rust /init\n");

    let mut uts = Utsname {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
        domainname: [0; 65],
    };
    // SAFETY: `uts` is a valid, writable `struct utsname`.
    let r = unsafe { syscall(SYS_UNAME, [&raw mut uts as usize, 0, 0, 0, 0]) };
    if r == 0 {
        puts(b"uname: ");
        for field in [&uts.sysname, &uts.nodename, &uts.release, &uts.version] {
            put_field(field);
            puts(b" ");
        }
        put_field(&uts.machine);
        puts(b"\n");
    } else {
        puts(b"uname failed: ");
        put_dec(r);
        puts(b"\n");
    }

    puts(b"Powering off.\n");
    // SAFETY: `sync` and `reboot` take only plain integers.
    let r = unsafe {
        syscall(SYS_SYNC, [0; 5]);
        syscall(
            SYS_REBOOT,
            [
                LINUX_REBOOT_MAGIC1,
                LINUX_REBOOT_MAGIC2,
                LINUX_REBOOT_CMD_POWER_OFF,
                0,
                0,
            ],
        )
    };
    // Only reached if the power off failed.
    puts(b"reboot(LINUX_REBOOT_CMD_POWER_OFF) failed: ");
    put_dec(r);
    puts(b"\n");
    exit(1)
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    puts(b"init: panic\n");
    exit(2)
}
