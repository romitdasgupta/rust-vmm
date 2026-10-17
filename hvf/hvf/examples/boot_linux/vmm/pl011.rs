// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! A minimal ARM PL011 UART: enough for Linux's earlycon and amba-pl011
//! drivers.
//!
//! Transmitted bytes go to a bounded queue that a `console-output` thread
//! writes to the output. A vCPU never does blocking I/O, and never holds the
//! register lock while the output is stuck (stdout piped into a paused
//! pager, say): it sees `FR.TXFF` once the queue is full, and the transmit
//! interrupt comes back when the queue drains. Received bytes come from
//! [`Pl011::receive`]. The interrupt line (`RIS & IMSC != 0`) is
//! level-triggered and driven through the callback given to
//! [`Pl011::new`], normally `Gic::set_spi`.

use std::collections::VecDeque;
use std::io::Write;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::thread;
use std::time::Duration;

use vm_device::DeviceMmio;
use vm_device::bus::{MmioAddress, MmioAddressOffset};

// Register offsets.
const DR: u64 = 0x000;
const RSR: u64 = 0x004;
const FR: u64 = 0x018;
const ILPR: u64 = 0x020;
const IBRD: u64 = 0x024;
const FBRD: u64 = 0x028;
const LCR_H: u64 = 0x02c;
const CR: u64 = 0x030;
const IFLS: u64 = 0x034;
const IMSC: u64 = 0x038;
const RIS: u64 = 0x03c;
const MIS: u64 = 0x040;
const ICR: u64 = 0x044;
const DMACR: u64 = 0x048;
const ID_BASE: u64 = 0xfe0;

// Flag register bits.
const FR_RXFE: u32 = 1 << 4;
const FR_TXFF: u32 = 1 << 5;
const FR_RXFF: u32 = 1 << 6;
const FR_TXFE: u32 = 1 << 7;

// Interrupt bits (RIS, MIS, IMSC, ICR).
const INT_RX: u32 = 1 << 4;
const INT_TX: u32 = 1 << 5;
const INT_RT: u32 = 1 << 6;

/// `CR.LBE`: loopback, transmitted bytes are received again.
const CR_LBE: u32 = 1 << 7;
/// `LCR_H.FEN`: FIFOs enabled.
const LCR_H_FEN: u32 = 1 << 4;

/// Depth of the hardware receive FIFO (PL011 revision r1p5).
const FIFO_DEPTH: usize = 16;
/// Bytes buffered beyond the FIFO before input is dropped.
const RX_BUFFER: usize = 4096;
/// Bytes the transmit queue holds; `FR.TXFF` is set while it is full.
const TX_BUFFER: usize = 64 * 1024;
/// The transmit interrupt is raised while the queue holds at most this many
/// bytes, like the FIFO level trigger of the real UART.
const TX_LOW: usize = TX_BUFFER / 2;

/// `UARTPeriphID0-3` and `UARTPCellID0-3`: an ARM PL011 r1p5.
const ID: [u8; 8] = [0x11, 0x10, 0x14, 0x00, 0x0d, 0xf0, 0x05, 0xb1];

/// The register state.
#[derive(Debug)]
struct Regs {
    rx: VecDeque<u8>,
    ris: u32,
    imsc: u32,
    cr: u32,
    lcr_h: u32,
    ilpr: u32,
    ibrd: u32,
    fbrd: u32,
    ifls: u32,
    dmacr: u32,
    /// The level last passed to the interrupt callback.
    irq_level: bool,
}

/// The output sink and interrupt callback of a [`Pl011`].
type Sink = Box<dyn Write + Send>;
type IrqLine = Box<dyn Fn(bool) + Send + Sync>;

/// The transmit queue, shared with the `console-output` thread.
#[derive(Debug, Default)]
struct TxQueue {
    state: Mutex<TxState>,
    /// Signalled when bytes are queued or the queue is closed.
    queued: Condvar,
    /// Signalled when everything queued has been written.
    drained: Condvar,
}

#[derive(Debug, Default)]
struct TxState {
    bytes: VecDeque<u8>,
    /// The output thread is writing bytes it took off the queue.
    writing: bool,
    /// A byte was queued above [`TX_LOW`], which cleared the transmit
    /// interrupt; raise it again once the queue drains.
    raise_tx: bool,
    /// The UART is gone: write what is queued, then stop.
    closed: bool,
}

impl TxQueue {
    fn lock(&self) -> MutexGuard<'_, TxState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// An emulated PL011.
pub struct Pl011 {
    /// Taken before `tx.state` when both are needed.
    regs: Mutex<Regs>,
    tx: Arc<TxQueue>,
    irq: IrqLine,
}

impl std::fmt::Debug for Pl011 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pl011").field("regs", &self.regs).finish()
    }
}

impl Drop for Pl011 {
    fn drop(&mut self) {
        self.tx.lock().closed = true;
        self.tx.queued.notify_all();
    }
}

/// The `console-output` thread: writes the queued bytes to `out` until the
/// UART is dropped.
fn write_output(uart: Weak<Pl011>, tx: Arc<TxQueue>, mut out: Sink) {
    let mut chunk = Vec::new();
    loop {
        let raise_tx = {
            let mut state = tx.lock();
            state.writing = false;
            if state.bytes.is_empty() {
                tx.drained.notify_all();
            }
            while state.bytes.is_empty() && !state.closed {
                state = tx
                    .queued
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
            if state.bytes.is_empty() {
                return;
            }
            chunk.clear();
            chunk.extend(state.bytes.drain(..));
            state.writing = true;
            std::mem::take(&mut state.raise_tx)
        };
        // The queue is empty again: tell a guest that waits for room.
        if raise_tx && let Some(uart) = uart.upgrade() {
            uart.tx_drained();
        }
        // The console is best effort: a closed stdout must not stop the
        // guest. Only this thread waits while the output is blocked.
        let _ = out.write_all(&chunk);
        let _ = out.flush();
    }
}

impl Pl011 {
    /// A UART in its reset state that writes to `out` from a
    /// `console-output` thread and drives its interrupt line with `irq`.
    pub fn new(out: Sink, irq: IrqLine) -> std::io::Result<Arc<Pl011>> {
        let tx = Arc::new(TxQueue::default());
        let uart = Arc::new(Pl011 {
            regs: Mutex::new(Regs {
                rx: VecDeque::new(),
                ris: 0,
                imsc: 0,
                cr: 0x300,
                lcr_h: 0,
                ilpr: 0,
                ibrd: 0,
                fbrd: 0,
                ifls: 0x12,
                dmacr: 0,
                irq_level: false,
            }),
            tx: tx.clone(),
            irq,
        });
        let weak = Arc::downgrade(&uart);
        thread::Builder::new()
            .name("console-output".into())
            .spawn(move || write_output(weak, tx, out))?;
        Ok(uart)
    }

    /// Waits up to `timeout` until every transmitted byte has been written
    /// to the output. Returns `false` if the output is still blocked.
    pub fn flush(&self, timeout: Duration) -> bool {
        let state = self.tx.lock();
        let (_state, wait) = self
            .tx
            .drained
            .wait_timeout_while(state, timeout, |s| s.writing || !s.bytes.is_empty())
            .unwrap_or_else(PoisonError::into_inner);
        !wait.timed_out()
    }

    /// The output thread emptied the queue: raise the transmit interrupt.
    fn tx_drained(&self) {
        let mut regs = self.lock();
        regs.ris |= INT_TX;
        self.update(&mut regs);
    }

    fn lock(&self) -> MutexGuard<'_, Regs> {
        self.regs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Queues input bytes for the guest.
    pub fn receive(&self, bytes: &[u8]) {
        let mut regs = self.lock();
        for &b in bytes {
            if regs.rx.len() < RX_BUFFER {
                regs.rx.push_back(b);
            }
        }
        self.update(&mut regs);
    }

    /// Queues one transmitted byte for the output (or sends it back to the
    /// receiver in loopback mode). Never blocks.
    fn transmit(&self, regs: &mut Regs, byte: u8) {
        if regs.cr & CR_LBE != 0 {
            // Bounded like `receive`, so a guest that loops bytes back
            // without reading them cannot grow the VMM's memory.
            if regs.rx.len() < RX_BUFFER {
                regs.rx.push_back(byte);
            }
            // The byte left the (empty) FIFO at once.
            regs.ris |= INT_TX;
            return;
        }
        let mut tx = self.tx.lock();
        if tx.bytes.len() >= TX_BUFFER {
            // The guest ignored FR.TXFF; the byte is lost, as on hardware.
            return;
        }
        tx.bytes.push_back(byte);
        if tx.bytes.len() <= TX_LOW {
            regs.ris |= INT_TX;
        } else {
            // Like a write that fills the FIFO past its trigger level.
            regs.ris &= !INT_TX;
            tx.raise_tx = true;
        }
        drop(tx);
        self.tx.queued.notify_one();
    }

    /// Recomputes the receive interrupts and the interrupt line. Called with
    /// the lock held, so the line changes in the same order as the state.
    fn update(&self, regs: &mut Regs) {
        if regs.rx.is_empty() {
            regs.ris &= !(INT_RX | INT_RT);
        } else {
            regs.ris |= INT_RX | INT_RT;
        }
        let level = regs.ris & regs.imsc != 0;
        if level != regs.irq_level {
            regs.irq_level = level;
            (self.irq)(level);
        }
    }

    fn fifo_depth(regs: &Regs) -> usize {
        if regs.lcr_h & LCR_H_FEN != 0 {
            FIFO_DEPTH
        } else {
            1
        }
    }

    /// Reads the 32-bit register at `offset` (a multiple of 4).
    fn read_reg(&self, regs: &mut Regs, offset: u64) -> u32 {
        match offset {
            DR => {
                let byte = regs.rx.pop_front().unwrap_or(0);
                self.update(regs);
                u32::from(byte)
            }
            FR => {
                let mut fr = 0;
                let queued = self.tx.lock().bytes.len();
                if queued == 0 {
                    fr |= FR_TXFE;
                }
                if queued >= TX_BUFFER {
                    fr |= FR_TXFF;
                }
                if regs.rx.is_empty() {
                    fr |= FR_RXFE;
                }
                if regs.rx.len() >= Self::fifo_depth(regs) {
                    fr |= FR_RXFF;
                }
                fr
            }
            ILPR => regs.ilpr,
            IBRD => regs.ibrd,
            FBRD => regs.fbrd,
            LCR_H => regs.lcr_h,
            CR => regs.cr,
            IFLS => regs.ifls,
            IMSC => regs.imsc,
            RIS => regs.ris,
            MIS => regs.ris & regs.imsc,
            DMACR => regs.dmacr,
            ID_BASE..=0xffc => u32::from(ID[((offset - ID_BASE) / 4) as usize]),
            // RSR, ICR (write-only) and reserved registers.
            _ => 0,
        }
    }

    /// Writes the 32-bit register at `offset` (a multiple of 4).
    fn write_reg(&self, regs: &mut Regs, offset: u64, value: u32) {
        match offset {
            DR => self.transmit(regs, value as u8),
            RSR => {}
            ILPR => regs.ilpr = value & 0xff,
            IBRD => regs.ibrd = value & 0xffff,
            FBRD => regs.fbrd = value & 0x3f,
            LCR_H => regs.lcr_h = value & 0xff,
            CR => regs.cr = value & 0xffff,
            IFLS => regs.ifls = value & 0x3f,
            IMSC => regs.imsc = value & 0x7ff,
            ICR => regs.ris &= !value,
            DMACR => regs.dmacr = value & 0x7,
            _ => {}
        }
        self.update(regs);
    }
}

impl DeviceMmio for Pl011 {
    fn mmio_read(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &mut [u8]) {
        let mut regs = self.lock();
        // Linux uses 16-bit (amba-pl011) or 32-bit (earlycon `mmio32`)
        // accesses; narrower ones read part of the 32-bit register.
        let value = self.read_reg(&mut regs, offset & !3) >> ((offset & 3) * 8);
        let bytes = u64::from(value).to_le_bytes();
        let len = data.len().min(8);
        data[..len].copy_from_slice(&bytes[..len]);
    }

    fn mmio_write(&self, _base: MmioAddress, offset: MmioAddressOffset, data: &[u8]) {
        if offset & 3 != 0 {
            return;
        }
        let mut bytes = [0u8; 4];
        let len = data.len().min(4);
        bytes[..len].copy_from_slice(&data[..len]);
        let mut regs = self.lock();
        self.write_reg(&mut regs, offset, u32::from_le_bytes(bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::mpsc;
    use std::time::Duration;

    const BASE: MmioAddress = MmioAddress(0x0900_0000);

    /// An output buffer the test can read back.
    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);

    impl Write for Buf {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(data);
            Ok(data.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn uart_with(out: Sink) -> (Arc<Pl011>, Arc<Mutex<Vec<bool>>>) {
        let levels = Arc::new(Mutex::new(Vec::new()));
        let l = levels.clone();
        let uart = Pl011::new(out, Box::new(move |level| l.lock().unwrap().push(level))).unwrap();
        (uart, levels)
    }

    fn uart() -> (Arc<Pl011>, Buf, Arc<Mutex<Vec<bool>>>) {
        let out = Buf::default();
        let (uart, levels) = uart_with(Box::new(out.clone()));
        (uart, out, levels)
    }

    /// An output that blocks in `write` until the test lets it go.
    struct Stuck {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
        out: Buf,
    }

    impl Write for Stuck {
        fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
            let _ = self.entered.send(());
            let _ = self.release.recv();
            self.out.write(data)
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn read32(uart: &Pl011, offset: u64) -> u32 {
        let mut data = [0u8; 4];
        uart.mmio_read(BASE, offset, &mut data);
        u32::from_le_bytes(data)
    }

    fn write32(uart: &Pl011, offset: u64, value: u32) {
        uart.mmio_write(BASE, offset, &value.to_le_bytes());
    }

    #[test]
    fn test_id() {
        let (uart, _, _) = uart();
        let id: Vec<u8> = (0..8).map(|i| read32(&uart, 0xfe0 + 4 * i) as u8).collect();
        assert_eq!(id, ID);
        // A 16-bit read, as amba-pl011 does.
        let mut data = [0u8; 2];
        uart.mmio_read(BASE, CR, &mut data);
        assert_eq!(u16::from_le_bytes(data), 0x300);
    }

    #[test]
    fn test_tx_and_interrupts() {
        let (uart, out, levels) = uart();
        assert_eq!(read32(&uart, FR), FR_TXFE | FR_RXFE);
        write32(&uart, DR, u32::from(b'h'));
        uart.mmio_write(BASE, DR, b"i");
        assert!(uart.flush(Duration::from_secs(5)));
        assert_eq!(*out.0.lock().unwrap(), b"hi");
        assert_eq!(read32(&uart, RIS), INT_TX);
        // Unmasking TX raises the line; clearing it lowers it again.
        write32(&uart, IMSC, INT_TX | INT_RX);
        assert_eq!(read32(&uart, MIS), INT_TX);
        write32(&uart, ICR, INT_TX);
        assert_eq!(*levels.lock().unwrap(), [true, false]);
    }

    #[test]
    fn test_rx() {
        let (uart, _, levels) = uart();
        write32(&uart, IMSC, INT_RX | INT_RT);
        uart.receive(b"ab");
        assert_eq!(read32(&uart, FR) & FR_RXFE, 0);
        assert_eq!(read32(&uart, MIS), INT_RX | INT_RT);
        // Clearing RX while data is left keeps it asserted.
        write32(&uart, ICR, INT_RX | INT_RT);
        assert_eq!(read32(&uart, RIS), INT_RX | INT_RT);
        assert_eq!(read32(&uart, DR), u32::from(b'a'));
        assert_eq!(read32(&uart, DR), u32::from(b'b'));
        assert_eq!(read32(&uart, FR) & FR_RXFE, FR_RXFE);
        assert_eq!(read32(&uart, RIS), 0);
        assert_eq!(*levels.lock().unwrap(), [true, false]);
    }

    #[test]
    fn test_blocked_output() {
        let (entered_tx, entered) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let out = Buf::default();
        let (uart, levels) = uart_with(Box::new(Stuck {
            entered: entered_tx,
            release: release_rx,
            out: out.clone(),
        }));
        write32(&uart, IMSC, INT_TX);
        // The output thread takes the first byte and blocks writing it.
        write32(&uart, DR, u32::from(b'a'));
        entered.recv().unwrap();
        assert!(!uart.flush(Duration::from_millis(10)));

        // Writes, input and register reads still complete; the queue fills
        // up, which sets TXFF and drops the transmit interrupt.
        for _ in 0..TX_BUFFER {
            write32(&uart, DR, u32::from(b'b'));
        }
        uart.receive(b"x");
        assert_eq!(read32(&uart, FR) & (FR_TXFF | FR_TXFE), FR_TXFF);
        assert_eq!(read32(&uart, RIS) & INT_TX, 0);
        // A byte written while TXFF is set is lost.
        write32(&uart, DR, u32::from(b'c'));

        // Once the output moves again, the queue drains and the transmit
        // interrupt comes back.
        drop(release);
        assert!(uart.flush(Duration::from_secs(5)));
        assert_eq!(read32(&uart, FR) & (FR_TXFF | FR_TXFE), FR_TXFE);
        assert_eq!(read32(&uart, RIS) & INT_TX, INT_TX);
        assert_eq!(*levels.lock().unwrap(), [true, false, true]);
        let out = out.0.lock().unwrap();
        assert_eq!(out.len(), 1 + TX_BUFFER);
        assert!(!out.contains(&b'c'));
    }

    #[test]
    fn test_loopback() {
        let (uart, out, _) = uart();
        write32(&uart, CR, 0x301 | CR_LBE);
        write32(&uart, DR, 0x55);
        assert!(out.0.lock().unwrap().is_empty());
        assert_eq!(read32(&uart, DR), 0x55);

        // Bytes the guest does not read back are dropped beyond RX_BUFFER.
        for _ in 0..RX_BUFFER + 16 {
            write32(&uart, DR, 0x66);
        }
        let mut looped = 0;
        while read32(&uart, FR) & FR_RXFE == 0 {
            assert_eq!(read32(&uart, DR), 0x66);
            looped += 1;
        }
        assert_eq!(looped, RX_BUFFER);
    }
}
