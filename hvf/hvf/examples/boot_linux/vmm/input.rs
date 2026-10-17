// Copyright 2026 The rust-vmm authors.
// SPDX-License-Identifier: Apache-2.0 OR BSD-3-Clause

//! The `console-input` thread, and the `Ctrl-A` escape of the console.
//!
//! The thread waits with `poll` for the input or for a stop request, and
//! reads only when the input is readable. [`InputThread`] stops and joins
//! it when dropped, so that it never outlives the VMM run that started it:
//! a thread left blocked in `read` would take the next input of a later
//! run (and its `Ctrl-A x`) and throw it away. Waiting with `poll` also
//! copes with an input that has `O_NONBLOCK` set, where `read` fails with
//! `EAGAIN` instead of waiting.

use std::fs::File;
use std::io::{ErrorKind, Read, Write};
use std::ops::ControlFlow;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::thread::{self, JoinHandle};

/// `Ctrl-A`, the escape key of the console.
pub const CTRL_A: u8 = 0x01;

/// Decodes the console escapes: `Ctrl-A x` quits, `Ctrl-A Ctrl-A` sends a
/// `Ctrl-A`, and `Ctrl-A` followed by anything else sends both bytes.
#[derive(Debug, Default)]
pub struct Escape {
    pending: bool,
}

impl Escape {
    /// Appends the bytes of `input` meant for the guest to `out`. Returns
    /// `true` at `Ctrl-A x`, ignoring the rest of `input`.
    pub fn filter(&mut self, input: &[u8], out: &mut Vec<u8>) -> bool {
        for &b in input {
            if self.pending {
                self.pending = false;
                match b {
                    b'x' | b'X' => return true,
                    CTRL_A => out.push(CTRL_A),
                    _ => out.extend_from_slice(&[CTRL_A, b]),
                }
            } else if b == CTRL_A {
                self.pending = true;
            } else {
                out.push(b);
            }
        }
        false
    }
}

/// A running `console-input` thread. Dropping it stops the thread and
/// waits for it.
#[derive(Debug)]
pub struct InputThread {
    /// Written to (or closed) to stop the thread.
    stop: Option<File>,
    handle: Option<JoinHandle<()>>,
}

/// Creates a pipe; returns the read and the write end.
fn pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    // SAFETY: `fds` is valid for writing two file descriptors.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: `pipe` returned two new file descriptors that nothing else
    // owns.
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

/// Waits until `input` or `stop` is readable (or hung up). Returns `true`
/// for `input`, `false` when the thread must stop.
fn wait(input: &File, stop: &OwnedFd) -> std::io::Result<bool> {
    let mut fds = [
        libc::pollfd {
            fd: stop.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: input.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        // SAFETY: `fds` is a valid array of two `pollfd`s.
        if unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) } >= 0 {
            break;
        }
        let e = std::io::Error::last_os_error();
        if e.kind() != ErrorKind::Interrupted {
            return Err(e);
        }
    }
    if fds[0].revents != 0 {
        return Ok(false);
    }
    // `POLLNVAL` on the input is left to `read`: macOS's `poll` does not
    // support device files and reports `POLLNVAL` for them, `/dev/null`
    // included, where `read` then sees the end of the input. A closed
    // file descriptor still fails in `read`, with `EBADF`.
    Ok(true)
}

impl InputThread {
    /// Starts a `console-input` thread that passes what it reads from
    /// `input` to `deliver`, until `deliver` breaks, the input ends or
    /// fails (reported to `closed`), or the returned value is dropped.
    pub fn spawn(
        input: OwnedFd,
        mut deliver: impl FnMut(&[u8]) -> ControlFlow<()> + Send + 'static,
        closed: impl FnOnce(std::io::Error) + Send + 'static,
    ) -> std::io::Result<InputThread> {
        let (stop_rx, stop_tx) = pipe()?;
        let mut input = File::from(input);
        let handle = thread::Builder::new()
            .name("console-input".into())
            .spawn(move || {
                let mut buf = [0u8; 256];
                let error = loop {
                    match wait(&input, &stop_rx) {
                        Ok(true) => {}
                        Ok(false) => return,
                        Err(e) => break e,
                    }
                    let n = match input.read(&mut buf) {
                        Ok(0) => return,
                        Ok(n) => n,
                        // A spurious wakeup, or an input with O_NONBLOCK:
                        // wait again.
                        Err(e)
                            if matches!(
                                e.kind(),
                                ErrorKind::Interrupted | ErrorKind::WouldBlock
                            ) =>
                        {
                            continue;
                        }
                        Err(e) => break e,
                    };
                    if deliver(&buf[..n]).is_break() {
                        return;
                    }
                };
                closed(error);
            })?;
        Ok(InputThread {
            stop: Some(File::from(stop_tx)),
            handle: Some(handle),
        })
    }
}

impl Drop for InputThread {
    fn drop(&mut self) {
        if let Some(mut stop) = self.stop.take() {
            // Either the byte or the closed pipe wakes the thread.
            let _ = stop.write_all(&[0]);
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn test_escape() {
        let mut escape = Escape::default();
        let mut out = Vec::new();
        assert!(!escape.filter(b"ab\x01\x01c\x01", &mut out));
        assert_eq!(out, b"ab\x01c");
        // The escape spans two reads.
        out.clear();
        assert!(!escape.filter(b"d", &mut out));
        assert_eq!(out, b"\x01d");
        out.clear();
        assert!(escape.filter(b"e\x01xf", &mut out));
        assert_eq!(out, b"e");
    }

    /// Spawns an input thread on a pipe that sends what it reads to the
    /// returned channel.
    fn spawn_on_pipe() -> (InputThread, File, File, mpsc::Receiver<Vec<u8>>) {
        let (rx, tx) = pipe().unwrap();
        let reader = File::from(rx.try_clone().unwrap());
        let (sent, received) = mpsc::channel();
        let thread = InputThread::spawn(
            rx,
            move |bytes| {
                let _ = sent.send(bytes.to_vec());
                ControlFlow::Continue(())
            },
            |e| panic!("input closed: {e}"),
        )
        .unwrap();
        (thread, File::from(tx), reader, received)
    }

    #[test]
    fn test_stop_leaves_later_input() {
        let (thread, mut tx, mut reader, received) = spawn_on_pipe();
        tx.write_all(b"one").unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            b"one"
        );
        // Once dropped, the thread is gone and reads nothing more: what
        // comes next is left for the next reader of the input.
        drop(thread);
        tx.write_all(b"two").unwrap();
        let mut buf = [0u8; 3];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"two");
        assert!(received.try_recv().is_err());
    }

    #[test]
    fn test_nonblocking_input() {
        let (rx, tx) = pipe().unwrap();
        // SAFETY: plain `fcntl` calls on a file descriptor this test owns.
        unsafe {
            let flags = libc::fcntl(rx.as_raw_fd(), libc::F_GETFL);
            assert_eq!(
                libc::fcntl(rx.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK),
                0
            );
        }
        let (sent, received) = mpsc::channel();
        let thread = InputThread::spawn(
            rx,
            move |bytes| {
                let _ = sent.send(bytes.to_vec());
                ControlFlow::Continue(())
            },
            |e| panic!("input closed: {e}"),
        )
        .unwrap();
        // Nothing to read yet: the thread must wait, not give up.
        std::thread::sleep(Duration::from_millis(50));
        let mut tx = File::from(tx);
        tx.write_all(b"late").unwrap();
        assert_eq!(
            received.recv_timeout(Duration::from_secs(5)).unwrap(),
            b"late"
        );
        drop(thread);
    }

    #[test]
    fn test_eof_and_break_end_the_thread() {
        let (thread, tx, _reader, received) = spawn_on_pipe();
        drop(tx);
        // The thread ends by itself at EOF; dropping it still works.
        assert!(received.recv_timeout(Duration::from_secs(5)).is_err());
        drop(thread);

        let (rx, tx) = pipe().unwrap();
        let thread = InputThread::spawn(rx, |_| ControlFlow::Break(()), |e| panic!("{e}")).unwrap();
        File::from(tx).write_all(b"x").unwrap();
        drop(thread);
    }

    #[test]
    fn test_dev_null_ends_quietly() {
        // macOS's `poll` reports `POLLNVAL` for `/dev/null`; the thread
        // must still end at its EOF without reporting an error.
        let input = OwnedFd::from(File::open("/dev/null").unwrap());
        let (closed_tx, closed_rx) = mpsc::channel();
        let (delivered_tx, delivered_rx) = mpsc::channel::<()>();
        let mut thread = InputThread::spawn(
            input,
            move |_| {
                let _ = delivered_tx.send(());
                ControlFlow::Continue(())
            },
            move |e| {
                let _ = closed_tx.send(e.to_string());
            },
        )
        .unwrap();
        // The thread ends by itself, without delivering or reporting.
        thread.handle.take().unwrap().join().unwrap();
        assert_eq!(closed_rx.try_recv().ok(), None);
        assert!(delivered_rx.try_recv().is_err());
        drop(thread);
    }
}
