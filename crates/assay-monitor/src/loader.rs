use std::sync::{Arc, Mutex};
use aya::{
    maps::{HashMap as AyaHashMap, LpmTrie, RingBuf},
    programs::{Lsm, TracePoint},
    Ebpf, Btf,
};
use aya::maps::lpm_trie::Key;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use crate::events::{self, EventStream};
use crate::MonitorError;
use assay_policy::tiers::CompiledPolicy;

#[cfg(target_os = "linux")]
pub struct LinuxMonitor {
    bpf: Arc<Mutex<Ebpf>>,
    links: Vec<MonitorLink>,
}

#[cfg(target_os = "linux")]
enum MonitorLink {
    #[allow(dead_code)]
    TracePoint(#[allow(dead_code)] aya::programs::trace_point::TracePointLink),
    #[allow(dead_code)]
    Lsm(#[allow(dead_code)] aya::programs::lsm::LsmLink),
    #[allow(dead_code)]
    KProbe(#[allow(dead_code)] aya::programs::kprobe::KProbeLink),
}

#[cfg(target_os = "linux")]
impl LinuxMonitor {
    pub fn new(ebpf_data: &[u8]) -> Result<Self, MonitorError> {
        let bpf = Ebpf::load(ebpf_data).map_err(|e| MonitorError::LoadError(e.to_string()))?;
        Ok(Self {
            bpf: Arc::new(Mutex::new(bpf)),
            links: Vec::new(),
        })
    }

    pub fn load_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, MonitorError> {
        let path_ref = path.as_ref();
        let data = std::fs::read(path_ref).map_err(|e| MonitorError::FileError(format!("{}: {}", path_ref.display(), e)))?;
        Self::new(&data)
    }

    pub fn load_bytes(bytes: &[u8]) -> Result<Self, MonitorError> {
        Self::new(bytes)
    }

    pub fn set_config(&mut self, config: &std::collections::HashMap<u32, u32>) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf.map_mut("CONFIG").ok_or(MonitorError::MapNotFound { name: "CONFIG" })?;
        let mut hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;

        for (k, v) in config {
            hm.insert(k, v, 0)?;
        }

        // Verification Loop
        for (k, v) in config {
            let actual = hm.get(k, 0)?;
            if actual != *v {
                return Err(MonitorError::ConfigVerification {
                    key: *k,
                    expected: *v,
                    got: actual,
                });
            }
        }
        Ok(())
    }

    pub fn configure_defaults(&mut self) -> Result<(), MonitorError> {
        // Example: Set default offsets for common kernels
        let config = std::collections::HashMap::from([
            (0, 24), // openat filename offset
            (1, 24), // connect sockaddr offset
        ]);
        self.set_config(&config)
    }

    pub fn set_monitor_all(&mut self, enabled: bool) -> Result<(), MonitorError> {
        let val = if enabled { 1 } else { 0 };
        let config = std::collections::HashMap::from([
             (100, val), // KEY_MONITOR_ALL
        ]);
        self.set_config(&config)
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

    pub fn attach(&mut self) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();

        // Initialize aya-log to capture kernel info! messages
        if let Err(e) = aya_log::EbpfLogger::init(&mut bpf) {
            eprintln!("Warning: Failed to initialize BPF logger: {}", e);
        }

        // 1. Tracepoints
        if let Some(prog) = bpf.program_mut("assay_monitor_openat") {
             if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                  tp.load()?;
                  let link_id = tp.attach("syscalls", "sys_enter_openat")?;
                  let link = tp.take_link(link_id)?;
                  self.links.push(MonitorLink::TracePoint(link));
                  println!("DEBUG: Attached Tracepoint sys_enter_openat");
             }
        }
        if let Some(prog) = bpf.program_mut("assay_monitor_openat2") {
             if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                  tp.load()?;
                  let link_id = tp.attach("syscalls", "sys_enter_openat2")?;
                  let link = tp.take_link(link_id)?;
                  self.links.push(MonitorLink::TracePoint(link));
                  println!("DEBUG: Attached Tracepoint sys_enter_openat2");
             }
        }
        if let Some(prog) = bpf.program_mut("assay_monitor_connect") {
             if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                  tp.load()?;
                  let link_id = tp.attach("syscalls", "sys_enter_connect")?;
                  let link = tp.take_link(link_id)?;
                  self.links.push(MonitorLink::TracePoint(link));
                  println!("DEBUG: Attached Tracepoint sys_enter_connect");
             }
        }
        if let Some(prog) = bpf.program_mut("assay_monitor_fork") {
             if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                  tp.load()?;
                  match tp.attach("syscalls", "sys_enter_fork") {
                      Ok(link_id) => {
                          if let Ok(link) = tp.take_link(link_id) {
                              self.links.push(MonitorLink::TracePoint(link));
                              println!("DEBUG: Attached Tracepoint sys_enter_fork");
                          }
                      },
                      Err(e) => eprintln!("WARN: Failed to attach sys_enter_fork: {}", e),
                  }
             }
        }

        // 2. LSM
        {
             if let Some(prog) = bpf.program_mut("file_open_lsm") {
                  if let Ok(lsm) = TryInto::<&mut Lsm>::try_into(&mut *prog) {
                      let btf = Btf::from_sys_fs()?;
                      lsm.load("file_open", &btf)?;
                      let link_id = lsm.attach()?;
                      let link = lsm.take_link(link_id)?;
                      self.links.push(MonitorLink::Lsm(link));
                      println!("DEBUG: Attached LSM file_open");
                  }
             }
        }

        Ok(())
    }

    pub fn set_tier1_rules(&mut self, compiled: &CompiledPolicy) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();

        // 1. File Path Exact Matches
        if let Some(map) = bpf.map_mut("DENY_PATHS_EXACT") {
            let mut hm: AyaHashMap<_, u64, u32> = AyaHashMap::try_from(map)?;
            for (key, rule_id) in compiled.tier1.file_exact_entries() {
                 hm.insert(key, rule_id, 0)?;
            }
        }

        // 2. Inode Exact Matches (SOTA)
        if let Some(map) = bpf.map_mut("DENY_INO") {
            use assay_common::InodeKey;
            let mut hm: AyaHashMap<_, InodeKey, u32> = AyaHashMap::try_from(map)?;
            for rule in compiled.tier1.inode_deny_exact.iter() {
                let key = InodeKey {
                    dev: rule.dev, // u32
                    pad: 0,
                    ino: rule.ino,
                    gen: rule.gen,
                    _pad2: 0,
                };
                hm.insert(key, rule.rule_id, 0)?;
                println!("DEBUG: Inserted Inode Rule: dev={} gen={} ino={} rule_id={}", rule.dev, rule.gen, rule.ino, rule.rule_id);

                // SOTA Hardening: Always insert default-generation (0) rule as fallback
                // This covers cases where:
                // 1. Kernel logic falls back to checking gen=0
                // 2. Filesystems report varying generations
                // For a DENY rule, "Fail Closed" means we block the Inode ID even if generation mismatches (risk of collision is acceptable for safety).
                if rule.gen != 0 {
                    let key_fallback = InodeKey {
                        dev: rule.dev,
                        pad: 0,
                        ino: rule.ino,
                        gen: 0,
                        _pad2: 0,
                    };
                    hm.insert(key_fallback, rule.rule_id, 0)?;
                    println!("DEBUG: Inserted Fallback Rule: dev={} gen=0 ino={} rule_id={}", rule.dev, rule.ino, rule.rule_id);
                }
            }
        }

        if let Some(map) = bpf.map_mut("DENY_PATHS_PREFIX") {
            let mut hm: AyaHashMap<_, u64, [u32; 2]> = AyaHashMap::try_from(map)?;
            for (hash, (len, rule_id)) in compiled.tier1.file_prefix_entries() {
                hm.insert(hash, [len, rule_id], 0)?;
            }
        }

        if let Some(map) = bpf.map_mut("CIDR_RULES_V4") {
            let mut trie: LpmTrie<_, [u8; 4], u32> = LpmTrie::try_from(map)?;
            for (prefix_len, addr, action) in compiled.tier1.cidr_v4_entries() {
                trie.insert(&Key::new(prefix_len, addr), action as u32, 0)?;
            }
        }

        Ok(())
    }

    pub fn attach_network_cgroup(&mut self, _cgroup_file: &std::fs::File) -> Result<(), MonitorError> {
        // Stub for compatibility
        Ok(())
    }

    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        let bpf_shared = self.bpf.clone();
        let (tx, rx) = mpsc::channel(1024);

        std::thread::spawn(move || {
            'outer: loop {
                {
                    let mut bpf = bpf_shared.lock().unwrap();

                    // Poll Tracepoint Events
                    if let Some(map) = bpf.map_mut("EVENTS") {
                        if let Ok(mut ring_buf) = RingBuf::try_from(map) {
                            while let Some(item) = ring_buf.next() {
                                if item.len() == 0 { continue; }
                                let ev = events::parse_event(&item);
                                if tx.blocking_send(ev).is_err() { break 'outer; }
                            }
                        }
                    }

                    // Poll LSM Events
                    if let Some(map) = bpf.map_mut("LSM_EVENTS") {
                        if let Ok(mut ring_buf) = RingBuf::try_from(map) {
                             while let Some(item) = ring_buf.next() {
                                 if item.len() == 0 { continue; }
                                 let ev = events::parse_event(&item);
                                 if tx.blocking_send(ev).is_err() { break 'outer; }
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
