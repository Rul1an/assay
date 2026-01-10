#![cfg(target_os = "linux")]

use crate::{events, EventStream, MonitorError};
use aya::{
    maps::{ring_buf::RingBuf, HashMap as AyaHashMap},
    programs::TracePoint,
    Bpf,
};
use std::{path::Path, time::Duration};
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
        openat.attach("syscalls", "sys_enter_openat")?;

        let connect: &mut TracePoint = self
            .bpf
            .program_mut("assay_monitor_connect")
            .ok_or(MonitorError::MapNotFound {
                name: "program assay_monitor_connect",
            })?
            .try_into()?;

        connect.load()?;
        connect.attach("syscalls", "sys_enter_connect")?;

        Ok(())
    }

    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        let map = self
            .bpf
            .map_mut("EVENTS")
            .ok_or(MonitorError::MapNotFound { name: "EVENTS" })?;

        let mut rb = RingBuf::try_from(map)?;

        // Manual thread spawn with channel (as per 'start now' instruction)
        let (tx, rx) = mpsc::channel(1024);

        std::thread::spawn(move || {
            loop {
                let poll_res = rb.poll(Duration::from_millis(200), |data| {
                    events::send_parsed(&tx, data);
                });

                if let Err(e) = poll_res {
                    // Send error and exit
                    let _ = tx.blocking_send(Err(MonitorError::from(e)));
                    break;
                }

                // If tx closed, stop polling
                if tx.is_closed() {
                    break;
                }
            }
        });

        Ok(ReceiverStream::new(rx))
    }
}
