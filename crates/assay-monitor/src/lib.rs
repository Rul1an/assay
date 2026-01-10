mod error;
pub use error::MonitorError;

pub mod events;

#[cfg(target_os = "linux")]
mod loader;

use assay_common::MonitorEvent;

// We use the alias from events, or define it here.
pub type EventStream = tokio_stream::wrappers::ReceiverStream<Result<MonitorEvent, MonitorError>>;

pub struct Monitor {
    #[cfg(target_os = "linux")]
    inner: loader::LinuxMonitor,

    #[cfg(not(target_os = "linux"))]
    _stub: (),
}

impl Monitor {
    /// Load eBPF object bytes from file (Linux). Non-Linux returns NotSupported.
    pub fn load_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, MonitorError> {
        #[cfg(target_os = "linux")]
        {
            let inner = loader::LinuxMonitor::load_file(path)?;
            Ok(Self { inner })
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = path;
            Err(MonitorError::NotSupported)
        }
    }

    /// Load eBPF object bytes from memory (Linux). Non-Linux returns NotSupported.
    pub fn load_bytes(bytes: &[u8]) -> Result<Self, MonitorError> {
        #[cfg(target_os = "linux")]
        {
            let inner = loader::LinuxMonitor::load_bytes(bytes)?;
            Ok(Self { inner })
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = bytes;
            Err(MonitorError::NotSupported)
        }
    }

    /// Configure monitored PIDs by writing to MONITORED_PIDS map.
    pub fn set_monitored_pids(&mut self, pids: &[u32]) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_monitored_pids(pids);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = pids;
            Err(MonitorError::NotSupported)
        }
    }

    /// Attach probes/tracepoints.
    pub fn attach(&mut self) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.attach();

        #[cfg(not(target_os = "linux"))]
        Err(MonitorError::NotSupported)
    }

    /// Start reading events from the RingBuf and return a stream.
    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.listen();

        #[cfg(not(target_os = "linux"))]
        Err(MonitorError::NotSupported)
    }
}
