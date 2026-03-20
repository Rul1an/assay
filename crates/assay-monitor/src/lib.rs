mod error;
pub use error::MonitorError;

pub mod events;
pub mod tree;

#[cfg(target_os = "linux")]
mod loader;
#[cfg(target_os = "linux")]
pub mod tracepoint;

use assay_common::MonitorEvent;

// We use the alias from events, or define it here.
pub type EventStream = tokio_stream::wrappers::ReceiverStream<Result<MonitorEvent, MonitorError>>;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MonitorStatsSnapshot {
    pub tracepoint_events_emitted: u32,
    pub tracepoint_ringbuf_dropped: u32,
    pub lsm_events_emitted: u32,
    pub lsm_ringbuf_dropped: u32,
    pub socket_checks: u64,
    pub socket_blocked_cidr: u64,
    pub socket_blocked_port: u64,
    pub socket_allowed: u64,
    pub socket_events_emitted: u64,
    pub socket_ringbuf_dropped: u64,
}

impl MonitorStatsSnapshot {
    pub fn total_ringbuf_dropped(&self) -> u64 {
        u64::from(self.tracepoint_ringbuf_dropped)
            + u64::from(self.lsm_ringbuf_dropped)
            + self.socket_ringbuf_dropped
    }

    pub fn has_ringbuf_pressure(&self) -> bool {
        self.total_ringbuf_dropped() > 0
    }
}

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

    pub fn configure_defaults(&mut self) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.configure_defaults();

        #[cfg(not(target_os = "linux"))]
        Ok(())
    }

    pub fn set_monitored_cgroups(&mut self, cgroups: &[u64]) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_monitored_cgroups(cgroups);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = cgroups;
            Err(MonitorError::NotSupported)
        }
    }

    pub fn set_tier1_rules(
        &mut self,
        compiled: &assay_policy::tiers::CompiledPolicy,
    ) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_tier1_rules(compiled);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = compiled;
            Ok(())
        }
    }

    pub fn attach_network_cgroup(
        &mut self,
        cgroup_file: &std::fs::File,
    ) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.attach_network_cgroup(cgroup_file);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = cgroup_file;
            Err(MonitorError::NotSupported)
        }
    }

    pub fn set_monitor_all(&mut self, enabled: bool) -> Result<(), MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.set_monitor_all(enabled);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = enabled;
            Ok(())
        }
    }

    pub fn get_config_u32(&mut self, key: u32) -> Result<u32, MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.get_config_u32(key);

        #[cfg(not(target_os = "linux"))]
        {
            let _ = key;
            Ok(0)
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

    pub fn snapshot_stats(&mut self) -> Result<MonitorStatsSnapshot, MonitorError> {
        #[cfg(target_os = "linux")]
        return self.inner.snapshot_stats();

        #[cfg(not(target_os = "linux"))]
        Err(MonitorError::NotSupported)
    }
}

#[cfg(test)]
mod tests {
    use super::MonitorStatsSnapshot;

    #[test]
    fn monitor_stats_snapshot_reports_ringbuf_pressure() {
        let stats = MonitorStatsSnapshot {
            tracepoint_ringbuf_dropped: 2,
            lsm_ringbuf_dropped: 1,
            socket_ringbuf_dropped: 3,
            ..Default::default()
        };

        assert!(stats.has_ringbuf_pressure());
        assert_eq!(stats.total_ringbuf_dropped(), 6);
    }
}
