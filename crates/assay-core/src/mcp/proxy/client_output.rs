//! The client-facing output of one proxy session, shared by both directions under the session mutex.
//!
//! Writes go through the session's own handle to the process's stdout (a duplicate of fd 1 on
//! Unix, a duplicated handle on Windows), never through `std::io::Stdout`. `Stdout` takes the
//! process-wide stdout lock for every write and flush, so a host that holds `stdout().lock()` on
//! another thread could block a proxy writer while it holds the session mutex, then the other
//! direction waiting for that mutex, and then the join that ends the session.
//!
//! A write to a client that stops reading must also end with the session. On Unix the write
//! waits for room with a short poll that re-checks the session's stop flag. On Windows the
//! session cancels the relay's blocked write (`cancel_blocked_io`), and the next write sees the
//! stop flag.
//!
//! One exception: a Windows console keeps `Stdout`, which converts UTF-8 to the console's
//! UTF-16; raw bytes on a console handle would not. A console is never an MCP client's pipe.

use std::{
    fs::File,
    io::{self, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub(super) struct ClientOutput {
    sink: Sink,
    stopped: Arc<AtomicBool>,
}

enum Sink {
    Own(File),
    #[cfg(windows)]
    Console(io::Stdout),
    #[cfg(not(any(unix, windows)))]
    Std(io::Stdout),
}

impl ClientOutput {
    /// The process's stdout, for one session that stops when `stopped` is set.
    pub(super) fn stdout(stopped: Arc<AtomicBool>) -> io::Result<Self> {
        #[cfg(unix)]
        let sink = {
            use std::os::fd::AsFd as _;
            Sink::Own(File::from(io::stdout().as_fd().try_clone_to_owned()?))
        };
        #[cfg(windows)]
        let sink = {
            use std::io::IsTerminal as _;
            use std::os::windows::io::AsHandle as _;
            let stdout = io::stdout();
            if stdout.is_terminal() {
                Sink::Console(stdout)
            } else {
                Sink::Own(File::from(stdout.as_handle().try_clone_to_owned()?))
            }
        };
        #[cfg(not(any(unix, windows)))]
        let sink = Sink::Std(io::stdout());
        Ok(Self { sink, stopped })
    }

    #[cfg(test)]
    pub(super) fn from_file(file: File, stopped: Arc<AtomicBool>) -> Self {
        Self {
            sink: Sink::Own(file),
            stopped,
        }
    }
}

fn stopped_error() -> io::Error {
    // Never `Interrupted`: `write_all` retries that kind.
    io::Error::other("client output stopped with the session before the client accepted it")
}

impl Write for ClientOutput {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        if self.stopped.load(Ordering::Acquire) {
            return Err(stopped_error());
        }
        match &mut self.sink {
            Sink::Own(file) => write_own(file, buf, &self.stopped),
            #[cfg(windows)]
            Sink::Console(stdout) => stdout.write(buf),
            #[cfg(not(any(unix, windows)))]
            Sink::Std(stdout) => stdout.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.sink {
            // Unbuffered: every accepted byte is already with the client.
            Sink::Own(_) => Ok(()),
            #[cfg(windows)]
            Sink::Console(stdout) => stdout.flush(),
            #[cfg(not(any(unix, windows)))]
            Sink::Std(stdout) => stdout.flush(),
        }
    }
}

/// Waits until the client accepts data, re-checking the stop flag every 25 ms, then writes at
/// most `PIPE_BUF` bytes, which a pipe that reported room accepts without blocking.
#[cfg(unix)]
fn write_own(file: &mut File, buf: &[u8], stopped: &AtomicBool) -> io::Result<usize> {
    use nix::{
        errno::Errno,
        poll::{poll, PollFd, PollFlags},
    };
    loop {
        if stopped.load(Ordering::Acquire) {
            return Err(stopped_error());
        }
        match poll(&mut [PollFd::new(&*file, PollFlags::POLLOUT)], 25) {
            Ok(0) | Err(Errno::EINTR) => continue,
            Ok(_) => break,
            Err(e) => return Err(io::Error::from_raw_os_error(e as i32)),
        }
    }
    file.write(&buf[..buf.len().min(libc::PIPE_BUF)])
}

/// A blocked write is cancelled by the session (`cancel_blocked_io`) when it stops.
#[cfg(not(unix))]
fn write_own(file: &mut File, buf: &[u8], _stopped: &AtomicBool) -> io::Result<usize> {
    file.write(buf)
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    fn pipe_output(stopped: Arc<AtomicBool>) -> (io::PipeReader, ClientOutput) {
        let (reader, writer) = io::pipe().unwrap();
        #[cfg(unix)]
        let file = File::from(std::os::fd::OwnedFd::from(writer));
        #[cfg(windows)]
        let file = File::from(std::os::windows::io::OwnedHandle::from(writer));
        (reader, ClientOutput::from_file(file, stopped))
    }

    #[test]
    fn delivers_everything_to_a_slow_reader() {
        use std::io::Read as _;
        let (mut reader, mut out) = pipe_output(Arc::new(AtomicBool::new(false)));
        let frame: Vec<u8> = (0..256 * 1024).map(|i| (i % 251) as u8).collect();
        let expected = frame.clone();
        let slow = std::thread::spawn(move || {
            let mut got = Vec::new();
            let mut buf = [0u8; 4096];
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1));
                match reader.read(&mut buf).unwrap() {
                    0 => return got,
                    n => got.extend_from_slice(&buf[..n]),
                }
            }
        });
        out.write_all(&frame).unwrap();
        out.flush().unwrap();
        drop(out);
        assert!(
            slow.join().unwrap() == expected,
            "slow reader got different bytes"
        );
    }

    /// Unix only: on Windows the session cancels a blocked write instead.
    #[cfg(unix)]
    #[test]
    fn returns_once_stopped_while_the_client_does_not_read() {
        let stopped = Arc::new(AtomicBool::new(false));
        let (_unread, mut out) = pipe_output(stopped.clone());
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            stopped.store(true, Ordering::Release);
        });
        let (done, finished) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let frame = vec![b'x'; 4 * 1024 * 1024];
            let _ = done.send(out.write_all(&frame));
        });
        let result = finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the write did not return after the session stopped");
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Other);
    }

    #[test]
    fn writes_while_a_host_holds_the_process_stdout_lock() {
        // The session mutex guards this type for both directions; neither may need the
        // process-wide stdout lock to write.
        use std::io::Read as _;
        let (mut reader, out) = pipe_output(Arc::new(AtomicBool::new(false)));
        let session = Arc::new(std::sync::Mutex::new(out));
        let host_lock = io::stdout().lock();
        let (done, finished) = std::sync::mpsc::channel();
        let writer_session = session.clone();
        std::thread::spawn(move || {
            let mut out = writer_session.lock().unwrap();
            let result = out
                .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":7,\"error\":{}}\n")
                .and_then(|()| out.flush());
            let _ = done.send(result.is_ok());
        });
        let wrote = finished.recv_timeout(std::time::Duration::from_secs(5));
        drop(host_lock);
        assert_eq!(
            wrote,
            Ok(true),
            "a session write waited on the process stdout lock"
        );
        drop(session);
        let mut got = String::new();
        reader.read_to_string(&mut got).unwrap();
        assert!(got.contains("\"id\":7"));
    }

    #[test]
    fn a_stopped_session_refuses_further_writes() {
        let stopped = Arc::new(AtomicBool::new(true));
        let (_reader, mut out) = pipe_output(stopped);
        assert_eq!(
            out.write(b"x").unwrap_err().kind(),
            io::ErrorKind::Other,
            "a write after the stop must fail without retrying as Interrupted"
        );
    }
}
