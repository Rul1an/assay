#![cfg(target_os = "linux")]

use crate::{events, EventStream, MonitorError};
use aya::{
    maps::{ring_buf::RingBuf, HashMap as AyaHashMap, LpmTrie, lpm_trie::Key},
    programs::{TracePoint, Lsm, CgroupSockAddr},
    Bpf, Btf,
};
use std::path::Path;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use assay_policy::tiers::CompiledPolicy;

pub enum MonitorLink {
    #[allow(dead_code)]
    TracePoint(aya::programs::trace_point::TracePointLinkId),
    #[allow(dead_code)]
    Lsm(aya::programs::lsm::LsmLinkId),
    #[allow(dead_code)]
    CgroupSockAddr(aya::programs::cgroup_sock_addr::CgroupSockAddrLinkId),
}

pub struct LinuxMonitor {
    bpf: std::sync::Arc<std::sync::Mutex<Bpf>>,
    links: Vec<MonitorLink>,
}

impl LinuxMonitor {
    pub fn load_file<P: AsRef<Path>>(path: P) -> Result<Self, MonitorError> {
        let bpf = Bpf::load_file(path)?;
        Ok(Self { bpf: std::sync::Arc::new(std::sync::Mutex::new(bpf)), links: Vec::new() })
    }

    pub fn load_bytes(bytes: &[u8]) -> Result<Self, MonitorError> {
        let bpf = Bpf::load(bytes)?;
        Ok(Self { bpf: std::sync::Arc::new(std::sync::Mutex::new(bpf)), links: Vec::new() })
    }

    pub fn set_monitored_pids(&mut self, pids: &[u32]) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf.map_mut("MONITORED_PIDS").ok_or(MonitorError::MapNotFound { name: "MONITORED_PIDS" })?;
        let mut hm: AyaHashMap<_, u32, u8> = AyaHashMap::try_from(map)?;
        for &pid in pids {
            hm.insert(pid, 1, 0)?;
        }
        Ok(())
    }

    pub fn set_monitored_cgroups(&mut self, cgroups: &[u64]) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf.map_mut("MONITORED_CGROUPS").ok_or(MonitorError::MapNotFound { name: "MONITORED_CGROUPS" })?;
        let mut hm: AyaHashMap<_, u64, u8> = AyaHashMap::try_from(map)?;
        for &cg in cgroups {
            hm.insert(cg, 1, 0)?;
        }
        Ok(())
    }

    pub fn set_config(&mut self, config: &std::collections::HashMap<u32, u32>) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf.map_mut("CONFIG").ok_or(MonitorError::MapNotFound { name: "CONFIG" })?;
        let mut hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;
        for (&k, &v) in config {
            hm.insert(k, v, 0)?;
        }
        Ok(())
    }

    pub fn set_tier1_rules(&mut self, compiled: &CompiledPolicy) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        if let Some(map) = bpf.map_mut("DENY_PATHS_EXACT") {
            let mut hm: AyaHashMap<_, u64, u32> = AyaHashMap::try_from(map)?;
            for (hash, rule_id) in compiled.tier1.file_exact_entries() {
                hm.insert(hash, rule_id, 0)?;
            }
        }

        if let Some(map) = bpf.map_mut("DENY_PATHS_PREFIX") {
            let mut hm: AyaHashMap<_, u64, [u32; 2]> = AyaHashMap::try_from(map)?;
            for (hash, (len, rule_id)) in compiled.tier1.file_prefix_entries() {
                hm.insert(hash, [len, rule_id], 0)?;
            }
        }

        if let Some(map) = bpf.map_mut("CIDR_RULES_V4") {
            let mut trie: LpmTrie<_, [u8; 4], u8> = LpmTrie::try_from(map)?;
            for (prefix_len, addr, action) in compiled.tier1.cidr_v4_entries() {
                trie.insert(&Key::new(prefix_len, addr), action, 0)?;
            }
        }

        if let Some(map) = bpf.map_mut("DENY_PORTS") {
            let mut hm: AyaHashMap<_, u16, u32> = AyaHashMap::try_from(map)?;
            for (port, rule_id) in compiled.tier1.port_deny_entries() {
                hm.insert(port, rule_id, 0)?;
            }
        }

        Ok(())
    }

    pub fn configure_defaults(&mut self) -> Result<(), MonitorError> {
        let config = crate::tracepoint::TracepointResolver::resolve_default_offsets();
        self.set_config(&config)
    }

    pub fn set_monitor_all(&mut self, enabled: bool) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        if let Some(map) = bpf.map_mut("CONFIG_LSM") {
            let mut hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;
            hm.insert(0, if enabled { 1 } else { 0 }, 0)?;
        }

        if let Some(map) = bpf.map_mut("CONFIG") {
            let mut hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;
            hm.insert(100, if enabled { 1 } else { 0 }, 0)?;
        }
        Ok(())
    }

    pub fn attach(&mut self) -> Result<(), MonitorError> {
        self.attach_tracepoints()?;
        self.attach_lsm()?;
        self.attach_socket_hooks()?;
        Ok(())
    }

    fn attach_tracepoints(&mut self) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let names = ["assay_monitor_openat", "assay_monitor_openat2", "assay_monitor_connect"];
        let syscalls = [("syscalls", "sys_enter_openat"), ("syscalls", "sys_enter_openat2"), ("syscalls", "sys_enter_connect")];

        for (name, (category, syscall)) in names.iter().zip(syscalls.iter()) {
            if let Some(prog) = bpf.program_mut(name) {
                let tp: &mut TracePoint = prog.try_into()?;
                tp.load()?;
                let link = tp.attach(category, syscall)?;
                self.links.push(MonitorLink::TracePoint(link));
            }
        }
        Ok(())
    }

    fn attach_lsm(&mut self) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let btf = Btf::from_sys_fs().ok();
        if let Some(prog) = bpf.program_mut("file_open_lsm") {
            let lsm: &mut Lsm = prog.try_into()?;
            if let Some(btf) = &btf {
                lsm.load("file_open", btf)?;
                let link = lsm.attach()?;
                self.links.push(MonitorLink::Lsm(link));
            }
        }
        Ok(())
    }

    pub fn attach_network_cgroup(&mut self, cgroup_file: &std::fs::File) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let progs = [
            ("connect4_hook", "assay_monitor_connect4"),
            ("connect6_hook", "assay_monitor_connect6"),
        ];

        for (name, _) in progs {
            if let Some(prog) = bpf.program_mut(name) {
                let hooks: &mut CgroupSockAddr = prog.try_into()?;
                hooks.load()?;
                let link = hooks.attach(cgroup_file)?;
                self.links.push(MonitorLink::CgroupSockAddr(link));
            }
        }
        Ok(())
    }

    fn attach_socket_hooks(&mut self) -> Result<(), MonitorError> {
        let cgroup_file = std::fs::File::open("/sys/fs/cgroup")
            .map_err(|e| MonitorError::Io(e))?;
        self.attach_network_cgroup(&cgroup_file)
    }

    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        let bpf_shared = self.bpf.clone();
        let (tx, rx) = mpsc::channel(1024);

        std::thread::spawn(move || {
            loop {
                {
                    let mut bpf = bpf_shared.lock().unwrap();

                    // Poll Tracepoint Events
                    if let Some(map) = bpf.map_mut("EVENTS") {
                        if let Ok(mut ring_buf) = RingBuf::try_from(map) {
                            while let Some(item) = ring_buf.next() {
                                let ev = events::parse_event(&item);
                                if tx.blocking_send(ev).is_err() { return; }
                            }
                        }
                    }

                    // Poll LSM Events
                    if let Some(map) = bpf.map_mut("LSM_EVENTS") {
                        if let Ok(mut ring_buf) = RingBuf::try_from(map) {
                            while let Some(item) = ring_buf.next() {
                                let ev = events::parse_event(&item);
                                if tx.blocking_send(ev).is_err() { return; }
                            }
                        }
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        });

        Ok(ReceiverStream::new(rx))
    }
}
