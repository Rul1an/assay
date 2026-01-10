use thiserror::Error;

#[derive(Debug, Error)]
pub enum MonitorError {
    #[error("runtime monitor is not supported on this OS")]
    NotSupported,

    #[cfg(target_os = "linux")]
    #[error("aya error: {0}")]
    Aya(#[from] aya::BpfError),

    #[cfg(target_os = "linux")]
    #[error("map '{name}' not found")]
    MapNotFound { name: &'static str },

    #[error("invalid event size (got={got}, need={need})")]
    InvalidEvent { got: usize, need: usize },

    #[error("ringbuf reader thread terminated")]
    ReaderDied,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}
