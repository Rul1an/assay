use crate::config_flags::{apply_dedup_open_paths, apply_emit_observed_connect, FlagConfigSink};
use crate::events::{self, EventStream};
use crate::object_abi::{verify_object_abi_marker, REBUILD_EBPF_GUIDANCE};
use crate::probes::{
    connect4_update, send_update, Connect4Fault, ModeUpdate, ProbeAttachment, ProbeProgram,
    SendFault, EGRESS_PEER_PROBE, EXPECTED_PROBES,
};
use crate::{MonitorError, MonitorStatsSnapshot};
// The `MONITOR_STAT_*` / `SOCKET_STAT_*` keys are deliberately not imported here: `snapshot_stats`
// names no key, so it cannot get one wrong. They belong to `crate::project_snapshot`.
use assay_common::{CidrRuleValue, KEY_EMIT_INODE_RESOLVED, KEY_MONITOR_ALL};
use assay_policy::tiers::CompiledPolicy;
use aya::maps::lpm_trie::Key;
use aya::{
    maps::{Array as AyaArray, HashMap as AyaHashMap, LpmTrie, RingBuf},
    programs::{CgroupAttachMode, CgroupSockAddr, Lsm, ProgramError, TracePoint},
    Btf, Ebpf,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

fn unknown_probe() -> MonitorError {
    MonitorError::ProgramSet {
        detail: "unknown program table row".into(),
        guidance: REBUILD_EBPF_GUIDANCE,
    }
}

fn attach_kernel_lacks_point(err: &ProgramError) -> bool {
    // ENOENT=2, EOPNOTSUPP=95 on Linux.
    match err {
        ProgramError::SyscallError(sy) => matches!(sy.io_error.raw_os_error(), Some(2 | 95)),
        ProgramError::TracePointError(aya::programs::trace_point::TracePointError::FileError {
            io_error,
            ..
        }) => matches!(io_error.raw_os_error(), Some(2 | 95)),
        _ => false,
    }
}

#[cfg(target_os = "linux")]
fn attach_send_tracepoint(
    bpf: &mut Ebpf,
    row: &ProbeProgram,
) -> Result<aya::programs::trace_point::TracePointLink, (SendFault, String)> {
    let prog = bpf.program_mut(row.elf_name).ok_or_else(|| {
        (
            SendFault::MissingProgram,
            format!("{} is absent from the loaded eBPF object", row.elf_name),
        )
    })?;
    let tp = TryInto::<&mut TracePoint>::try_into(&mut *prog)
        .map_err(|error| (SendFault::WrongProgramKind, error.to_string()))?;
    tp.load()
        .map_err(|error| (SendFault::LoadFailed, error.to_string()))?;
    let link_id = tp.attach(row.tp().0, row.tp().1).map_err(|error| {
        (
            SendFault::AttachFailed {
                kernel_lacks_point: attach_kernel_lacks_point(&error),
            },
            error.to_string(),
        )
    })?;
    tp.take_link(link_id).map_err(|error| {
        (
            SendFault::AttachFailed {
                kernel_lacks_point: false,
            },
            error.to_string(),
        )
    })
}

#[cfg(target_os = "linux")]
/// One-shot, actionable warning emitted on the first event-size mismatch of a run. Further
/// mismatches are counted, not logged, so a stale object does not drown the log in per-event noise.
#[cfg(target_os = "linux")]
const STALE_OBJECT_WARNING: &str = "WARN: monitor event size mismatch: a ring-buffer record did \
not match the pinned MonitorEvent size. This almost always means a stale eBPF object (built from \
an older MonitorEvent layout) was loaded against a newer userspace decoder. Rebuild the eBPF \
object from the same commit as assay-monitor:\n  scripts/ci/install-ebpf-toolchain.sh\n  cargo \
xtask build-ebpf --release --no-docker\nMismatched records are dropped fail-closed and counted; \
further mismatches are not logged individually.";

pub struct LinuxMonitor {
    bpf: Arc<Mutex<Ebpf>>,
    links: Vec<MonitorLink>,
    /// Which probes actually attached, and which were skipped.
    ///
    /// `links` alone cannot answer this: `MonitorLink` records the program *kind*, and the handles
    /// are held for RAII only, never inspected. Every attach site here is guarded by
    /// `if let Some(prog)` / `if let Ok(tp)`, so a program that is missing from the object or fails
    /// to convert is skipped without a trace — which means an unattached probe and a probe that
    /// attached and saw nothing were previously indistinguishable. Any coverage artifact built on
    /// that would report silence as coverage, which is the failure the coverage descriptor exists to
    /// prevent.
    probe_attachment: ProbeAttachment,
    /// Userspace counter for ring-buffer records whose length did not match the pinned
    /// `MonitorEvent` size (almost always a stale eBPF object). Shared with the consumer thread
    /// spawned in `listen()` and read back in `snapshot_stats`.
    event_size_mismatch: Arc<AtomicU64>,
}

#[cfg(target_os = "linux")]
enum MonitorLink {
    #[allow(dead_code)]
    TracePoint(#[allow(dead_code)] aya::programs::trace_point::TracePointLink),
    #[allow(dead_code)]
    Lsm(#[allow(dead_code)] aya::programs::lsm::LsmLink),
    #[allow(dead_code)]
    KProbe(#[allow(dead_code)] aya::programs::kprobe::KProbeLink),
    #[allow(dead_code)]
    CgroupSockAddr(#[allow(dead_code)] aya::programs::cgroup_sock_addr::CgroupSockAddrLink),
}

#[cfg(target_os = "linux")]
impl FlagConfigSink for LinuxMonitor {
    fn put_flag(&mut self, key: u32, value: u32) -> Result<(), MonitorError> {
        let config = std::collections::HashMap::from([(key, value)]);
        self.set_config(&config)
    }
}

/// What the loader actually managed to attach, recorded at the attach sites themselves.
///
/// Names are the eBPF program names, so a reader can map an entry back to a source file in
/// `assay-ebpf` without guessing.
#[cfg(target_os = "linux")]
impl LinuxMonitor {
    pub fn new(ebpf_data: &[u8]) -> Result<Self, MonitorError> {
        // P1: named CONFIG object-ABI symbol before Aya load (never set_global).
        verify_object_abi_marker(ebpf_data)?;
        let bpf = Ebpf::load(ebpf_data).map_err(|e| MonitorError::LoadError(e.to_string()))?;
        // P2: exact Aya program-name set before any program load/attach.
        crate::program_set::compare_program_names(bpf.programs().map(|(name, _)| name))?;
        Ok(Self {
            bpf: Arc::new(Mutex::new(bpf)),
            links: Vec::new(),
            probe_attachment: ProbeAttachment::default(),
            event_size_mismatch: Arc::new(AtomicU64::new(0)),
        })
    }

    pub fn load_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self, MonitorError> {
        let path_ref = path.as_ref();
        let data = std::fs::read(path_ref)
            .map_err(|e| MonitorError::FileError(format!("{}: {}", path_ref.display(), e)))?;
        Self::new(&data)
    }

    pub fn load_bytes(bytes: &[u8]) -> Result<Self, MonitorError> {
        Self::new(bytes)
    }

    pub fn set_config(
        &mut self,
        config: &std::collections::HashMap<u32, u32>,
    ) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf
            .map_mut("CONFIG")
            .ok_or(MonitorError::MapNotFound { name: "CONFIG" })?;
        let mut hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;

        for (k, v) in config {
            hm.insert(*k, *v, 0)?;
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

    pub fn get_config_u32(&mut self, key: u32) -> Result<u32, MonitorError> {
        let bpf = self.bpf.lock().unwrap();
        let map = bpf
            .map("CONFIG")
            .ok_or(MonitorError::MapNotFound { name: "CONFIG" })?;
        let hm: AyaHashMap<_, u32, u32> = AyaHashMap::try_from(map)?;
        Ok(hm.get(&key, 0).unwrap_or(0))
    }

    pub fn configure_defaults(&mut self) -> Result<(), MonitorError> {
        let config = crate::tracepoint::TracepointResolver::resolve_default_offsets();
        self.set_config(&config)
    }

    pub fn set_monitor_all(&mut self, enabled: bool) -> Result<(), MonitorError> {
        let val = if enabled { 1 } else { 0 };
        let config = std::collections::HashMap::from([(KEY_MONITOR_ALL, val)]);
        self.set_config(&config)
    }

    /// Ask the kernel to emit an event for every ALLOWED connect, not only blocked ones.
    ///
    /// Off unless a run wants a peer set. The allow path is the hot one, so this is opt-in rather
    /// than default: a run that does not ask pays nothing, and its peer set is honestly empty
    /// instead of quietly partial.
    pub fn set_emit_observed_connect(&mut self, enabled: bool) -> Result<(), MonitorError> {
        apply_emit_observed_connect(self, enabled)
    }

    pub fn set_emit_inode_resolved(&mut self, enabled: bool) -> Result<(), MonitorError> {
        let val = if enabled { 1 } else { 0 };
        let config = std::collections::HashMap::from([(KEY_EMIT_INODE_RESOLVED, val)]);
        self.set_config(&config)
    }

    pub fn set_dedup_open_paths(&mut self, enabled: bool) -> Result<(), MonitorError> {
        apply_dedup_open_paths(self, enabled)
    }

    pub fn set_monitored_pids(&mut self, pids: &[u32]) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf
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
        let mut bpf = self.bpf.lock().unwrap();
        let map = bpf
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

    /// What actually attached this run, and what did not.
    ///
    /// This is the input a coverage descriptor needs: a surface with no attached probe is
    /// `not_observed`, and silence on it is not evidence of absence.
    #[must_use]
    pub fn probe_attachment(&self) -> &ProbeAttachment {
        &self.probe_attachment
    }

    pub fn attach(&mut self) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();

        // Initialize aya-log to capture kernel info! messages
        if let Err(e) = aya_log::EbpfLogger::init(&mut bpf) {
            eprintln!("Warning: Failed to initialize BPF logger: {}", e);
        }

        // 1. Tracepoints
        let r = ProbeProgram::by_elf("assay_monitor_openat").ok_or_else(unknown_probe)?;
        if let Some(prog) = bpf.program_mut(r.elf_name) {
            if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                tp.load()?;
                let link_id = tp.attach(r.tp().0, r.tp().1)?;
                let link = tp.take_link(link_id)?;
                self.probe_attachment.attached(r.surface_name);
                self.links.push(MonitorLink::TracePoint(link));
                println!("DEBUG: Attached Tracepoint sys_enter_openat");
            }
        }
        let r = ProbeProgram::by_elf("assay_monitor_openat2").ok_or_else(unknown_probe)?;
        if let Some(prog) = bpf.program_mut(r.elf_name) {
            if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                tp.load()?;
                let link_id = tp.attach(r.tp().0, r.tp().1)?;
                let link = tp.take_link(link_id)?;
                self.probe_attachment.attached(r.surface_name);
                self.links.push(MonitorLink::TracePoint(link));
                println!("DEBUG: Attached Tracepoint sys_enter_openat2");
            }
        }
        let r = ProbeProgram::by_elf("assay_monitor_openat_exit").ok_or_else(unknown_probe)?;
        if let Some(prog) = bpf.program_mut(r.elf_name) {
            if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                tp.load()?;
                match tp.attach(r.tp().0, r.tp().1) {
                    Ok(link_id) => {
                        if let Ok(link) = tp.take_link(link_id) {
                            self.probe_attachment.attached(r.surface_name);
                            self.links.push(MonitorLink::TracePoint(link));
                            println!("DEBUG: Attached Tracepoint sys_exit_openat");
                        }
                    }
                    Err(e) => {
                        self.probe_attachment.skipped(r.surface_name);
                        eprintln!("WARN: Failed to attach sys_exit_openat: {}", e);
                    }
                }
            }
        }
        let r = ProbeProgram::by_elf("assay_monitor_openat2_exit").ok_or_else(unknown_probe)?;
        if let Some(prog) = bpf.program_mut(r.elf_name) {
            if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                tp.load()?;
                match tp.attach(r.tp().0, r.tp().1) {
                    Ok(link_id) => {
                        if let Ok(link) = tp.take_link(link_id) {
                            self.probe_attachment.attached(r.surface_name);
                            self.links.push(MonitorLink::TracePoint(link));
                            println!("DEBUG: Attached Tracepoint sys_exit_openat2");
                        }
                    }
                    Err(e) => {
                        self.probe_attachment.skipped(r.surface_name);
                        eprintln!("WARN: Failed to attach sys_exit_openat2: {}", e);
                    }
                }
            }
        }
        let r = ProbeProgram::by_elf("assay_monitor_connect").ok_or_else(unknown_probe)?;
        if let Some(prog) = bpf.program_mut(r.elf_name) {
            if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                tp.load()?;
                let link_id = tp.attach(r.tp().0, r.tp().1)?;
                let link = tp.take_link(link_id)?;
                self.probe_attachment.attached(r.surface_name);
                self.links.push(MonitorLink::TracePoint(link));
                println!("DEBUG: Attached Tracepoint sys_enter_connect");
            }
        }
        for r in [
            ProbeProgram::by_elf("assay_monitor_sendto").ok_or_else(unknown_probe)?,
            ProbeProgram::by_elf("assay_monitor_sendmsg").ok_or_else(unknown_probe)?,
        ] {
            match attach_send_tracepoint(&mut bpf, r) {
                Ok(link) => {
                    self.probe_attachment.attached(r.surface_name);
                    self.links.push(MonitorLink::TracePoint(link));
                    println!("DEBUG: Attached Tracepoint {}", r.surface_name);
                }
                Err((fault, detail)) => {
                    self.probe_attachment
                        .record_attempt_failure(r.surface_name, send_update(fault));
                    eprintln!("WARN: Failed to attach {}: {}", r.surface_name, detail);
                }
            }
        }
        crate::probe_inventory_result(self.probe_attachment.finalize_mode_aware(false))?;
        let r = ProbeProgram::by_elf("assay_monitor_fork").ok_or_else(unknown_probe)?;
        if let Some(prog) = bpf.program_mut(r.elf_name) {
            if let Ok(tp) = TryInto::<&mut TracePoint>::try_into(&mut *prog) {
                tp.load()?;
                match tp.attach(r.tp().0, r.tp().1) {
                    Ok(link_id) => {
                        if let Ok(link) = tp.take_link(link_id) {
                            self.probe_attachment.attached(r.surface_name);
                            self.links.push(MonitorLink::TracePoint(link));
                            println!("DEBUG: Attached Tracepoint sys_enter_fork");
                        }
                    }
                    Err(e) => {
                        self.probe_attachment.skipped(r.surface_name);
                        eprintln!("WARN: Failed to attach sys_enter_fork: {}", e);
                    }
                }
            }
        }

        // 2. LSM
        {
            let r = ProbeProgram::by_elf("file_open_lsm").ok_or_else(unknown_probe)?;
            if let Some(prog) = bpf.program_mut(r.elf_name) {
                if let Ok(lsm) = TryInto::<&mut Lsm>::try_into(&mut *prog) {
                    let btf = Btf::from_sys_fs()?;
                    lsm.load(r.lsm(), &btf)?;
                    let link_id = lsm.attach()?;
                    let link = lsm.take_link(link_id)?;
                    self.probe_attachment.attached(r.surface_name);
                    self.links.push(MonitorLink::Lsm(link));
                    println!("DEBUG: Attached LSM file_open");
                }
            }
        }

        // Anything expected that did not attach is a named blind spot, not silence.
        self.probe_attachment.reconcile(EXPECTED_PROBES);
        Ok(())
    }

    pub fn set_tier1_rules(&mut self, compiled: &CompiledPolicy) -> Result<(), MonitorError> {
        // Validate before taking the BPF lock or touching any map. A mixed IPv4/IPv6 policy must
        // not leave a successfully-loaded IPv4 subset behind when the IPv6 half is unsupported.
        crate::validate_network_enforcement_support(compiled)?;

        let mut bpf = self.bpf.lock().unwrap();

        // 1. File Path Exact Matches
        if let Some(map) = bpf.map_mut("DENY_PATHS_EXACT") {
            let mut hm: AyaHashMap<_, u64, u32> = AyaHashMap::try_from(map)?;
            for (key, rule_id) in compiled.tier1.file_exact_entries() {
                hm.insert(key, rule_id, 0)?;
            }
        }

        // 2. Inode Exact Matches (SOTA)
        // 2. Inode Exact Matches (SOTA)
        if let Some(map) = bpf.map_mut("DENY_INO") {
            use assay_common::InodeKeyMap;
            let mut hm: AyaHashMap<_, InodeKeyMap, u32> = AyaHashMap::try_from(map)?;
            for rule in compiled.tier1.inode_deny_exact.iter() {
                // 1. Standard new_encode_dev format (already valid in rule.dev)
                let key_std = InodeKeyMap {
                    ino: rule.ino,
                    dev: rule.dev,
                    gen: rule.gen,
                };
                hm.insert(key_std, rule.rule_id, 0)?;
                println!(
                    "DEBUG: Inserted Inode Rule (Std): dev={} gen={} ino={} rule_id={}",
                    rule.dev, rule.gen, rule.ino, rule.rule_id
                );

                // 2. Alternate/Old encoding logic: (major << 20) | minor
                // We decode rule.dev first (which is new_encoded) to get major/minor back
                let major = (rule.dev >> 8) & 0xfff;
                let minor = (rule.dev & 0xff) | ((rule.dev >> 12) & 0xfff00);
                let dev_alt = (major << 20) | minor;

                let key_alt = InodeKeyMap {
                    ino: rule.ino,
                    dev: dev_alt,
                    gen: rule.gen,
                };
                hm.insert(key_alt, rule.rule_id, 0)?;
                println!("DEBUG: Inserted Inode Rule (Alt): dev={} (maj={} min={}) gen={} ino={} rule_id={}", dev_alt, major, minor, rule.gen, rule.ino, rule.rule_id);

                // SOTA Hardening: Always insert default-generation (0) rule as fallback
                // This covers cases where:
                // 1. Kernel logic falls back to checking gen=0
                // 2. Filesystems report varying generations
                // For a DENY rule, "Fail Closed" means we block the Inode ID even if generation mismatches (risk of collision is acceptable for safety).
                if rule.gen != 0 {
                    let key_fallback_std = InodeKeyMap {
                        ino: rule.ino,
                        dev: rule.dev,
                        gen: 0,
                    };
                    hm.insert(key_fallback_std, rule.rule_id, 0)?;
                    println!(
                        "DEBUG: Inserted Fallback Rule (Std): dev={} gen=0 ino={} rule_id={}",
                        rule.dev, rule.ino, rule.rule_id
                    );

                    let key_fallback_alt = InodeKeyMap {
                        ino: rule.ino,
                        dev: dev_alt,
                        gen: 0,
                    };
                    hm.insert(key_fallback_alt, rule.rule_id, 0)?;
                    println!(
                        "DEBUG: Inserted Fallback Rule (Alt): dev={} gen=0 ino={} rule_id={}",
                        dev_alt, rule.ino, rule.rule_id
                    );
                }
            }
        }

        if let Some(map) = bpf.map_mut("DENY_PATHS_PREFIX") {
            let mut hm: AyaHashMap<_, u64, [u32; 2]> = AyaHashMap::try_from(map)?;
            for (hash, (len, rule_id)) in compiled.tier1.file_prefix_entries() {
                hm.insert(hash, [len, rule_id], 0)?;
            }
        }

        // CIDR rules -> CIDR_RULES_V4. The value carries the action the hook branches
        // on and the id of the rule that produced it; the hook reports that id as the
        // matched rule, so both must cross the boundary. This is the one place the
        // compiler's u8 action is widened to the shared eBPF ABI type.
        if let Some(map) = bpf.map_mut("CIDR_RULES_V4") {
            let mut trie: LpmTrie<_, [u8; 4], CidrRuleValue> = LpmTrie::try_from(map)?;
            for entry in compiled.tier1.cidr_v4_entries() {
                trie.insert(
                    &Key::new(entry.prefix_len, entry.addr),
                    CidrRuleValue {
                        action: u32::from(entry.action),
                        rule_id: entry.rule_id,
                    },
                    0,
                )?;
            }
        }

        // Port deny rules -> DENY_PORTS map read by the connect4 hook. Without this, a port-based
        // egress deny rule compiles but never reaches the kernel, so the hook runs but never blocks.
        if let Some(map) = bpf.map_mut("DENY_PORTS") {
            let mut hm: AyaHashMap<_, u16, u32> = AyaHashMap::try_from(map)?;
            for (port, rule_id) in compiled.tier1.port_deny_entries() {
                hm.insert(port, rule_id, 0)?;
            }
        }

        println!("✅ Policy applied: tier1 inode rules loaded");
        Ok(())
    }

    /// Attach the connect4 cgroup_sock_addr enforcement program to the given cgroup v2 directory.
    ///
    /// IPv4/TCP egress only. The `DENY_PORTS` / `CIDR_RULES_V4` maps (populated by `set_tier1_rules`)
    /// decide which connects are blocked; with an empty rule set every connect falls through to allow,
    /// so attaching is safe even at the cgroup root. The connect tracepoint observation path is
    /// untouched, so observation-health reporting is unaffected by enforcement being active.
    pub fn attach_network_cgroup(
        &mut self,
        cgroup_file: &std::fs::File,
    ) -> Result<(), MonitorError> {
        let mut bpf = self.bpf.lock().unwrap();
        // Fail-closed. Inventory via connect4_update: missing→Unavailable, wrong-kind/load→Failed,
        // attach ENOENT/EOPNOTSUPP→Unsupported, other attach→Failed.
        let r = ProbeProgram::by_elf("connect4_hook").ok_or_else(unknown_probe)?;
        let Some(prog) = bpf.program_mut(r.elf_name) else {
            self.probe_attachment.record_mode(
                r.surface_name,
                connect4_update(Connect4Fault::MissingProgram),
            );
            return Err(MonitorError::EnforcementUnavailable(
                "connect4_hook program not present in eBPF object".into(),
            ));
        };
        let csa: &mut CgroupSockAddr = match TryInto::<&mut CgroupSockAddr>::try_into(&mut *prog) {
            Ok(csa) => csa,
            Err(e) => {
                self.probe_attachment.record_mode(
                    r.surface_name,
                    connect4_update(Connect4Fault::WrongProgramKind),
                );
                return Err(MonitorError::EnforcementUnavailable(format!(
                    "connect4_hook is not a CgroupSockAddr program: {e}"
                )));
            }
        };
        if let Err(e) = csa.load() {
            self.probe_attachment
                .record_mode(r.surface_name, connect4_update(Connect4Fault::LoadFailed));
            return Err(e.into());
        }
        let link_id = match csa.attach(cgroup_file, CgroupAttachMode::Single) {
            Ok(id) => id,
            Err(e) => {
                let lacks = attach_kernel_lacks_point(&e);
                self.probe_attachment.record_mode(
                    r.surface_name,
                    connect4_update(Connect4Fault::AttachFailed {
                        kernel_lacks_point: lacks,
                    }),
                );
                return Err(e.into());
            }
        };
        let link = csa.take_link(link_id).inspect_err(|_| {
            self.probe_attachment.record_mode(
                r.surface_name,
                connect4_update(Connect4Fault::AttachFailed {
                    kernel_lacks_point: false,
                }),
            );
        })?;
        self.probe_attachment.attached(r.surface_name);
        self.links.push(MonitorLink::CgroupSockAddr(link));
        println!("DEBUG: Attached cgroup connect4 egress enforcement");
        Ok(())
    }

    pub(crate) fn record_egress_failed(&mut self, reason: &'static str) {
        self.probe_attachment
            .record_mode(EGRESS_PEER_PROBE, ModeUpdate::Failed(reason));
    }

    /// Bind the two kernel stat arrays to the snapshot projection.
    ///
    /// Everything this method knows is which aya map each key space lives in. Which key feeds which
    /// field is `crate::project_snapshot`'s business, and it is total there: this cannot forget a
    /// field, because it names none. The eBPF side has always incremented the per-hook and honesty
    /// counters (`connect_events.rs`); a read that goes missing here is a silent zero downstream,
    /// indistinguishable from a clean run, which is why the projection is one call and not a list of
    /// separately deletable assignments.
    pub fn snapshot_stats(&mut self) -> Result<MonitorStatsSnapshot, MonitorError> {
        let bpf = self.bpf.lock().unwrap();

        let map = bpf
            .map("STATS")
            .ok_or(MonitorError::MapNotFound { name: "STATS" })?;
        let stats_array: AyaArray<_, u32> = AyaArray::try_from(map)?;

        let map = bpf.map("SOCKET_STATS").ok_or(MonitorError::MapNotFound {
            name: "SOCKET_STATS",
        })?;
        let socket_array: AyaArray<_, u64> = AyaArray::try_from(map)?;

        Ok(crate::project_snapshot(
            |key| stats_array.get(&key, 0).unwrap_or(0),
            |key| socket_array.get(&key, 0).unwrap_or(0),
            // Userspace-tracked: filled by the consumer thread in `listen()`, not a kernel map.
            self.event_size_mismatch.load(Ordering::Relaxed),
        ))
    }

    pub fn listen(&mut self) -> Result<EventStream, MonitorError> {
        let (tx, rx) = mpsc::channel(1024);
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let (mut events_ring_buf, mut lsm_ring_buf, mut socket_ring_buf) = {
            let mut bpf = self.bpf.lock().unwrap();
            let events_map = bpf
                .take_map("EVENTS")
                .ok_or(MonitorError::MapNotFound { name: "EVENTS" })?;
            let lsm_events_map = bpf
                .take_map("LSM_EVENTS")
                .ok_or(MonitorError::MapNotFound { name: "LSM_EVENTS" })?;
            let socket_events_map = bpf.take_map("SOCKET_EVENTS");
            (
                RingBuf::try_from(events_map)?,
                RingBuf::try_from(lsm_events_map)?,
                socket_events_map.map(RingBuf::try_from).transpose()?,
            )
        };

        let mismatch = Arc::clone(&self.event_size_mismatch);
        std::thread::spawn(move || {
            let mut ready = false;
            'outer: loop {
                // Poll Tracepoint Events. Keep the RingBuf object alive across
                // polls so its consumer position advances; recreating it would
                // replay the same kernel records and inflate runner-spike
                // event counts.
                while let Some(item) = events_ring_buf.next() {
                    if item.is_empty() {
                        continue;
                    }
                    let ev = events::parse_event(&item);
                    if matches!(&ev, Err(MonitorError::InvalidEvent { .. }))
                        && mismatch.fetch_add(1, Ordering::Relaxed) == 0
                    {
                        eprintln!("{STALE_OBJECT_WARNING}");
                    }
                    if tx.blocking_send(ev).is_err() {
                        break 'outer;
                    }
                }

                // Poll LSM Events with the same persistent-consumer discipline.
                while let Some(item) = lsm_ring_buf.next() {
                    if item.is_empty() {
                        continue;
                    }
                    let ev = events::parse_event(&item);
                    if matches!(&ev, Err(MonitorError::InvalidEvent { .. }))
                        && mismatch.fetch_add(1, Ordering::Relaxed) == 0
                    {
                        eprintln!("{STALE_OBJECT_WARNING}");
                    }
                    if tx.blocking_send(ev).is_err() {
                        break 'outer;
                    }
                }

                // Poll cgroup socket events and project them into type-20
                // MonitorEvent payloads so downstream evidence exporters retain
                // cgroup/rule binding fields instead of opaque hex.
                if let Some(socket_ring_buf) = socket_ring_buf.as_mut() {
                    while let Some(item) = socket_ring_buf.next() {
                        if item.is_empty() {
                            continue;
                        }
                        let ev = events::parse_socket_event(&item);
                        if matches!(&ev, Err(MonitorError::InvalidEvent { .. }))
                            && mismatch.fetch_add(1, Ordering::Relaxed) == 0
                        {
                            eprintln!("{STALE_OBJECT_WARNING}");
                        }
                        if tx.blocking_send(ev).is_err() {
                            break 'outer;
                        }
                    }
                }
                if !ready {
                    let _ = ready_tx.send(());
                    ready = true;
                }
                // Runner-spike fixtures can emit very short openat bursts.
                // Keep poll latency low so zero-drop capture does not depend
                // on an oversized kernel ring buffer.
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        });

        ready_rx
            .recv_timeout(std::time::Duration::from_millis(100))
            .map_err(|_| MonitorError::ReaderDied)?;

        Ok(ReceiverStream::new(rx))
    }
}

#[cfg(test)]
mod attach_error_tests {
    use super::attach_kernel_lacks_point;
    use aya::{
        programs::{trace_point::TracePointError, ProgramError},
        sys::SyscallError,
    };
    use std::io;

    fn syscall_error(errno: i32) -> ProgramError {
        ProgramError::SyscallError(SyscallError {
            call: "test_attach",
            io_error: io::Error::from_raw_os_error(errno),
        })
    }

    fn tracepoint_file_error(errno: i32) -> ProgramError {
        ProgramError::TracePointError(TracePointError::FileError {
            filename: "/tracefs/events/syscalls/test/id".into(),
            io_error: io::Error::from_raw_os_error(errno),
        })
    }

    #[test]
    fn missing_tracepoint_classification_covers_both_aya_error_paths() {
        let cases = [
            ("syscall ENOENT", syscall_error(2), true),
            ("syscall EOPNOTSUPP", syscall_error(95), true),
            ("tracepoint ENOENT", tracepoint_file_error(2), true),
            ("tracepoint EOPNOTSUPP", tracepoint_file_error(95), true),
            ("syscall EACCES", syscall_error(13), false),
            ("tracepoint EACCES", tracepoint_file_error(13), false),
            ("unrelated program error", ProgramError::NotLoaded, false),
        ];

        for (name, error, expected) in cases {
            assert_eq!(attach_kernel_lacks_point(&error), expected, "{name}");
        }
    }
}
