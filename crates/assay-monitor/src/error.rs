use thiserror::Error;

#[derive(Debug, Error)]
pub enum MonitorError {
    #[error("failed to load BPF object: {0}")]
    LoadError(String),

    #[error("failed to read BPF file: {0}")]
    FileError(String),

    #[error("runtime monitor is not supported on this OS")]
    NotSupported,

    #[cfg(target_os = "linux")]
    #[error("aya error: {0}")]
    Aya(#[from] aya::EbpfError),

    #[cfg(target_os = "linux")]
    #[error("map error: {0}")]
    Map(#[from] aya::maps::MapError),

    #[cfg(target_os = "linux")]
    #[error("program error: {0}")]
    Program(#[from] aya::programs::ProgramError),

    #[cfg(target_os = "linux")]
    #[error("btf error: {0}")]
    Btf(#[from] aya::BtfError),

    #[cfg(target_os = "linux")]
    #[error("map '{name}' not found")]
    MapNotFound { name: &'static str },

    #[error("config verification failed for key {key}: expected {expected}, got {got}")]
    ConfigVerification { key: u32, expected: u32, got: u32 },

    #[error("invalid event size (got={got}, need={need})")]
    InvalidEvent { got: usize, need: usize },

    #[error("ringbuf reader thread terminated")]
    ReaderDied,

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ringbuf error: {0}")]
    RingBuf(String),
}
