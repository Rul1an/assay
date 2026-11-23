//! Cancellable, bounded stdin for live stdio sessions. A stalled client may
//! hold its pipe open after the upstream has closed its response stream.

use super::unique_json::FrameDirection;
use std::{
    io::{self, BufRead},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};
use tokio::sync::mpsc;

type Line = io::Result<Option<String>>;

#[derive(Clone)]
pub struct StopInput {
    stopped: Arc<AtomicBool>,
    wake: mpsc::Sender<Line>,
}

impl StopInput {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.wake.try_send(Ok(None));
    }
}

pub struct ClientInput {
    lines: mpsc::Receiver<Line>,
    stop: StopInput,
    worker: Option<thread::JoinHandle<()>>,
}

impl ClientInput {
    pub fn open() -> io::Result<Self> {
        let (send, lines) = mpsc::channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = StopInput {
            stopped: stopped.clone(),
            wake: send.clone(),
        };
        let worker = thread::Builder::new()
            .name("assay-client-stdin".into())
            .spawn(move || {
                let result = pump(&stopped, &send);
                let _ = send.blocking_send(result.map(|()| None));
            })?;
        Ok(Self {
            lines,
            stop,
            worker: Some(worker),
        })
    }

    pub fn stop_handle(&self) -> StopInput {
        self.stop.clone()
    }

    pub fn next_blocking(&mut self) -> Line {
        if self.stop.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        self.lines.blocking_recv().unwrap_or(Ok(None))
    }

    pub async fn next_frame(&mut self) -> Line {
        if self.stop.stopped.load(Ordering::Acquire) {
            return Ok(None);
        }
        self.lines.recv().await.unwrap_or(Ok(None))
    }
}

fn pump(stopped: &AtomicBool, send: &mpsc::Sender<Line>) -> io::Result<()> {
    #[cfg(unix)]
    let mut reader = {
        use std::os::fd::AsFd;
        let file = std::fs::File::from(io::stdin().as_fd().try_clone_to_owned()?);
        io::BufReader::new(polled_file(file, stopped))
    };
    #[cfg(not(unix))]
    let mut reader = io::stdin().lock();
    pump_lines(&mut reader, stopped, send)
}

fn pump_lines(
    reader: &mut impl BufRead,
    stopped: &AtomicBool,
    send: &mpsc::Sender<Line>,
) -> io::Result<()> {
    while !stopped.load(Ordering::Acquire) {
        let mut line = String::new();
        if FrameDirection::Client.read_line(reader, &mut line)? == 0
            || stopped.load(Ordering::Acquire)
            || send.blocking_send(Ok(Some(line))).is_err()
        {
            break;
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn polled_file(file: std::fs::File, stopped: &AtomicBool) -> impl io::Read + '_ {
    PolledInput { file, stopped }
}

#[cfg(unix)]
struct PolledInput<'a> {
    file: std::fs::File,
    stopped: &'a AtomicBool,
}

#[cfg(unix)]
impl io::Read for PolledInput<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        use nix::{
            errno::Errno,
            poll::{poll, PollFd, PollFlags},
        };
        while !self.stopped.load(Ordering::Acquire) {
            match poll(&mut [PollFd::new(&self.file, PollFlags::POLLIN)], 25) {
                Ok(0) | Err(Errno::EINTR) => continue,
                Ok(_) => return self.file.read(buffer),
                Err(e) => return Err(io::Error::from_raw_os_error(e as i32)),
            }
        }
        Ok(0)
    }
}

impl Drop for ClientInput {
    fn drop(&mut self) {
        self.stop.stop();
        self.lines.close();
        if let Some(worker) = self.worker.take() {
            #[cfg(windows)]
            cancel_blocked_io(&worker);
            let _ = worker.join();
        }
    }
}

#[cfg(windows)]
pub(super) fn cancel_blocked_io<T>(worker: &thread::JoinHandle<T>) {
    use std::os::windows::io::AsRawHandle;
    while !worker.is_finished() {
        // SAFETY: the join handle owns this live thread handle until join.
        #[allow(unsafe_code)]
        unsafe {
            windows_sys::Win32::System::IO::CancelSynchronousIo(worker.as_raw_handle());
        }
        thread::sleep(std::time::Duration::from_millis(5));
    }
}
