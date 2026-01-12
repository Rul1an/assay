#![cfg(target_os = "linux")]

use crate::{events, EventStream, MonitorError};
use aya::{
    maps::{ring_buf::RingBuf, HashMap as AyaHashMap},
    programs::TracePoint,
    Bpf,
};
use std::path::Path;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

pub struct LinuxMonitor {
    bpf: Bpf,
}

impl LinuxMonitor {
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Self, MonitorError> {
        let bpf = Bpf::load_file(path)?;
        Ok(Self { bpf })
    }

    pub fn load_bytes(bytes: &[u8]) -> Result<Self, MonitorError> {
        let bpf = Bpf::load(bytes)?;
        Ok(Self { bpf })
    }

    pub fn set_monitored_pids(&mut self, pids: &[u32]) -> Result<(), MonitorError> {
        let map = self
            .bpf
            .map_mut("MONITORED_PIDS")
            .ok_or(MonitorError::MapNotFound {
                name: "MONITORED_PIDS",
            })?;

        let mut hm: AyaHashMap<_, u32, u8> = AyaHashMap::try_from(map)?;
        for &pid in pids {
            hm.insert(pid, 1, 0)?;
        }
        Ok(())
    }

    pub fn set_monitored_cgroups(&mut self, cgroups: &[u64]) -> Result<(), MonitorError> {
        let map = self
            .bpf
            .map_mut("MONITORED_CGROUPS")
            .ok_or(MonitorError::MapNotFound {
                name: "MONITORED_CGROUPS",
            })?;

        let mut hm: AyaHashMap<_, u64, u8> = AyaHashMap::try_from(map)?;
        for &cg in cgroups {
            hm.insert(cg, 1, 0)?;
        }
        Ok(())
    }

    pub fn set_config(&mut self, config: &std::collections::HashMap<u32, u32>) -> Result<(), MonitorError> {
        let map = self
            .bpf
            .map_mut("CONFIG")
            .ok_or(MonitorError::MapNotFound {
                name: "CONFIG",
            })?;

        let mut hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;
        for (&k, &v) in config {
            hm.insert(k, v, 0)?;
        }
        Ok(())
    }

    pub fn configure_defaults(&mut self) -> Result<(), MonitorError> {
        let config = crate::tracepoint::TracepointResolver::resolve_default_offsets();
        self.set_config(&config)
    }

    pub fn attach(&mut self) -> Result<(), MonitorError> {
        // Program names must match your ebpf #[tracepoint] function names.
        let openat: &mut TracePoint = self
            .bpf
            .program_mut("assay_monitor_openat")
            .ok_or(MonitorError::MapNotFound {
                name: "program assay_monitor_openat",
            })?
            .try_into()?;

        openat.load()?;
        openat.load()?;
        openat.attach("syscalls", "sys_enter_openat")?;

        // SOTA: Try to attach openat2 (best effort, modern kernels only)
        if let Some(openat2) = self.bpf.program_mut("assay_monitor_openat2") {
            if let Ok(mut link) = openat2.try_into() as Result<TracePoint, _> {
                let _ = link.load();
                // If this fails (kernel too old), we just continue
                let _ = link.attach("syscalls", "sys_enter_openat2");
            }
        }

        let connect: &mut TracePoint = self
            .bpf
            .program_mut("assay_monitor_connect")
            .ok_or(MonitorError::MapNotFound {
                name: "program assay_monitor_connect",
            })?
            .try_into()?;

        connect.load()?;
        connect.attach("syscalls", "sys_enter_connect")?;

        let fork: &mut TracePoint = self
            .bpf
            .program_mut("assay_monitor_fork")
            .ok_or(MonitorError::MapNotFound {
                name: "program assay_monitor_fork",
            })?
            .try_into()?;

        fork.load()?;
        fork.attach("sched", "sched_process_fork")?;

        Ok(())
    }

    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        // Take ownership of the map so we can move it into the thread
        let map = self
            .bpf
            .take_map("EVENTS")
            .ok_or(MonitorError::MapNotFound { name: "EVENTS" })?;

        let mut rb = RingBuf::try_from(map)?;

        // Manual thread spawn with channel
        let (tx, rx) = mpsc::channel(1024);

        std::thread::spawn(move || {
            // AssertUnwindSafe is required because RingBuf isn't strictly UnwindSafe,
            // but we are just polling it in a loop and if we panic we exit the thread anyway.
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                loop {
                    if tx.is_closed() {
                        break;
                    }

                    // Using Iterator interface for RingBuf
                    match rb.next() {
                        Some(item) => {
                            events::send_parsed(&tx, &item);
                        }
                        None => {
                            // Buffer is empty, wait a bit before polling again
                            std::thread::sleep(std::time::Duration::from_millis(50));
                            continue;
                        }
                    }
                }
            }));
        });

        Ok(ReceiverStream::new(rx))
    }
}
