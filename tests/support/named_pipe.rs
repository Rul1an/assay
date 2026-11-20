//! One payload offered on a named pipe to a reader that may never come.
//!
//! The pipe exists when [`NamedPipe::offer`] returns, so a reader can be started on its path. A
//! thread then waits for a reader, hands it the payload and closes its end, which is the end of
//! the file for that reader.
//!
//! Nothing here waits without a bound. The thread gives up when the limit passes or when the offer
//! is withdrawn, whichever comes first, and [`NamedPipe::finish`] withdraws it. A reader that ends
//! without opening the pipe, or that never ends, therefore cannot hold a test: call `finish` once
//! the reader is done, however it got there.
//!
//! On Unix the pipe is a FIFO in the given directory. On Windows it is a pipe under `\\.\pipe\`,
//! which lives nowhere on disk.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// How often the waiting thread looks whether the offer still stands.
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// What became of the payload.
#[derive(Debug, PartialEq, Eq)]
pub enum Fed {
    /// A reader opened the pipe and was handed the whole payload.
    Written,
    /// No reader opened the pipe before the limit passed or the offer was withdrawn.
    NoReader,
    /// The pipe could not be served.
    Failed(String),
}

/// A named pipe with a payload waiting on it.
pub struct NamedPipe {
    path: PathBuf,
    withdrawn: Arc<AtomicBool>,
    feeder: Option<JoinHandle<Fed>>,
}

impl NamedPipe {
    /// Creates the pipe and offers `payload` on it for at most `limit`.
    pub fn offer(dir: &Path, payload: &[u8], limit: Duration) -> Result<Self, String> {
        let (path, pipe) = platform::create(dir)?;
        let withdrawn = Arc::new(AtomicBool::new(false));
        let feeder = {
            let payload = payload.to_vec();
            let withdrawn = Arc::clone(&withdrawn);
            let deadline = Instant::now() + limit;
            thread::spawn(move || pipe.feed(&payload, deadline, &withdrawn))
        };
        Ok(Self {
            path,
            withdrawn,
            feeder: Some(feeder),
        })
    }

    /// The path a reader opens.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Withdraws the offer if nobody took it, and says what became of the payload.
    ///
    /// Returns within a few poll intervals: the waiting thread never blocks on the pipe.
    pub fn finish(mut self) -> Fed {
        self.withdrawn.store(true, Ordering::SeqCst);
        match self.feeder.take() {
            Some(feeder) => feeder
                .join()
                .unwrap_or_else(|_| Fed::Failed("the thread serving the pipe panicked".into())),
            None => Fed::Failed("the offer was already finished".into()),
        }
    }
}

impl Drop for NamedPipe {
    fn drop(&mut self) {
        // A test that fails before `finish` must not leave the thread waiting out its limit.
        self.withdrawn.store(true, Ordering::SeqCst);
    }
}

#[cfg(unix)]
mod platform {
    use super::{Fed, POLL_INTERVAL};
    use std::fs::OpenOptions;
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::thread;
    use std::time::Instant;

    pub(super) struct Pipe {
        path: PathBuf,
    }

    pub(super) fn create(dir: &Path) -> Result<(PathBuf, Pipe), String> {
        let path = dir.join("trace.pipe");
        nix::unistd::mkfifo(&path, nix::sys::stat::Mode::from_bits_truncate(0o600))
            .map_err(|error| format!("mkfifo {}: {error}", path.display()))?;
        Ok((path.clone(), Pipe { path }))
    }

    impl Pipe {
        pub(super) fn feed(self, payload: &[u8], deadline: Instant, withdrawn: &AtomicBool) -> Fed {
            loop {
                // Opening a FIFO for writing waits for a reader, for ever when none comes. Opened
                // without waiting, it answers ENXIO until a reader is there, and that can be
                // asked again.
                match OpenOptions::new()
                    .write(true)
                    .custom_flags(libc::O_NONBLOCK)
                    .open(&self.path)
                {
                    Ok(mut pipe) => {
                        return match pipe.write_all(payload) {
                            // Dropping `pipe` closes the writing end: the end of the file.
                            Ok(()) => Fed::Written,
                            Err(error) => Fed::Failed(format!("write to the pipe: {error}")),
                        };
                    }
                    Err(error) if error.raw_os_error() == Some(libc::ENXIO) => {}
                    Err(error) => {
                        return Fed::Failed(format!(
                            "open {} for writing: {error}",
                            self.path.display()
                        ));
                    }
                }
                if withdrawn.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    return Fed::NoReader;
                }
                thread::sleep(POLL_INTERVAL);
            }
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::{Fed, POLL_INTERVAL};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use tokio::io::AsyncWriteExt;
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
    use tokio::runtime::Runtime;

    /// Pipe names are global on Windows, so each one carries this process and a counter.
    static NEXT: AtomicU64 = AtomicU64::new(0);

    pub(super) struct Pipe {
        runtime: Runtime,
        server: NamedPipeServer,
    }

    pub(super) fn create(_dir: &Path) -> Result<(PathBuf, Pipe), String> {
        let name = format!(
            r"\\.\pipe\assay-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("start a runtime for {name}: {error}"))?;
        let server = {
            // The server end registers with the runtime it is created in.
            let _in_runtime = runtime.enter();
            ServerOptions::new()
                .first_pipe_instance(true)
                .create(&name)
                .map_err(|error| format!("create {name}: {error}"))?
        };
        Ok((PathBuf::from(name), Pipe { runtime, server }))
    }

    impl Pipe {
        pub(super) fn feed(self, payload: &[u8], deadline: Instant, withdrawn: &AtomicBool) -> Fed {
            let Pipe {
                runtime,
                mut server,
            } = self;
            runtime.block_on(async move {
                {
                    let connected = server.connect();
                    tokio::pin!(connected);
                    loop {
                        tokio::select! {
                            reader = &mut connected => match reader {
                                Ok(()) => break,
                                Err(error) => {
                                    return Fed::Failed(format!("wait for a reader: {error}"));
                                }
                            },
                            () = tokio::time::sleep(POLL_INTERVAL) => {
                                if withdrawn.load(Ordering::SeqCst) || Instant::now() >= deadline {
                                    return Fed::NoReader;
                                }
                            }
                        }
                    }
                }
                let left = deadline
                    .saturating_duration_since(Instant::now())
                    .max(Duration::from_secs(1));
                match tokio::time::timeout(left, server.write_all(payload)).await {
                    // Dropping `server` closes this end. The reader takes what was written and
                    // then finds the end of the file.
                    Ok(Ok(())) => Fed::Written,
                    Ok(Err(error)) => Fed::Failed(format!("write to the pipe: {error}")),
                    Err(_) => Fed::Failed("the payload was not taken within the limit".into()),
                }
            })
        }
    }
}
