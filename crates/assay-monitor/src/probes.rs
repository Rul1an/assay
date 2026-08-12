//! Probe attachment + mode-aware inventory (`not_requested`≠`unavailable`≠`failed`≠`unsupported`).
//! `PROBE_PROGRAMS` owns ELF names, surfaces, class, and attach spec; [`EXPECTED_PROBES`] is derived.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeClass {
    Always,
    Mode(ProbeCondition),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachSpec {
    Tp(&'static str, &'static str),
    Lsm(&'static str),
    Cgroup4,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeProgram {
    pub elf_name: &'static str,
    pub surface_name: &'static str,
    pub class: ProbeClass,
    pub attach: AttachSpec,
}

#[rustfmt::skip]
pub(crate) const PROBE_PROGRAMS: &[ProbeProgram] = &[
    ProbeProgram { elf_name: "assay_monitor_openat", surface_name: "sys_enter_openat", class: ProbeClass::Always, attach: AttachSpec::Tp("syscalls", "sys_enter_openat") },
    ProbeProgram { elf_name: "assay_monitor_openat2", surface_name: "sys_enter_openat2", class: ProbeClass::Always, attach: AttachSpec::Tp("syscalls", "sys_enter_openat2") },
    ProbeProgram { elf_name: "assay_monitor_openat_exit", surface_name: "sys_exit_openat", class: ProbeClass::Always, attach: AttachSpec::Tp("syscalls", "sys_exit_openat") },
    ProbeProgram { elf_name: "assay_monitor_openat2_exit", surface_name: "sys_exit_openat2", class: ProbeClass::Always, attach: AttachSpec::Tp("syscalls", "sys_exit_openat2") },
    ProbeProgram { elf_name: "assay_monitor_connect", surface_name: "sys_enter_connect", class: ProbeClass::Always, attach: AttachSpec::Tp("syscalls", "sys_enter_connect") },
    ProbeProgram { elf_name: "assay_monitor_sendto", surface_name: "sys_enter_sendto", class: ProbeClass::Mode(ProbeCondition::Unsupported), attach: AttachSpec::None },
    ProbeProgram { elf_name: "assay_monitor_sendmsg", surface_name: "sys_enter_sendmsg", class: ProbeClass::Mode(ProbeCondition::Unsupported), attach: AttachSpec::None },
    ProbeProgram { elf_name: "assay_monitor_fork", surface_name: "sys_enter_fork", class: ProbeClass::Always, attach: AttachSpec::Tp("syscalls", "sys_enter_fork") },
    ProbeProgram { elf_name: "file_open_lsm", surface_name: "lsm:file_open", class: ProbeClass::Always, attach: AttachSpec::Lsm("file_open") },
    ProbeProgram { elf_name: "connect4_hook", surface_name: "cgroup_sock_addr:connect4", class: ProbeClass::Mode(ProbeCondition::RequiresNetworkPolicy), attach: AttachSpec::Cgroup4 },
    ProbeProgram { elf_name: "connect6_hook", surface_name: "cgroup_sock_addr:connect6", class: ProbeClass::Mode(ProbeCondition::Unsupported), attach: AttachSpec::None },
];

#[rustfmt::skip]
const ALWAYS_N: usize = {
    let mut n = 0; let mut i = 0;
    while i < PROBE_PROGRAMS.len() {
        if matches!(PROBE_PROGRAMS[i].class, ProbeClass::Always) { n += 1; }
        i += 1;
    }
    n
};

#[rustfmt::skip]
const fn always_surfaces() -> [&'static str; ALWAYS_N] {
    let mut out = [""; ALWAYS_N]; let mut i = 0; let mut j = 0;
    while i < PROBE_PROGRAMS.len() {
        if matches!(PROBE_PROGRAMS[i].class, ProbeClass::Always) {
            out[j] = PROBE_PROGRAMS[i].surface_name; j += 1;
        }
        i += 1;
    }
    out
}

pub const EXPECTED_PROBES: &[&str] = &always_surfaces();

pub const EGRESS_PEER_PROBE: &str = "cgroup_sock_addr:connect4";

#[cfg(any(test, target_os = "linux"))]
#[rustfmt::skip]
impl ProbeProgram {
    pub(crate) fn by_elf(elf: &str) -> Option<&'static Self> {
        PROBE_PROGRAMS.iter().find(|p| p.elf_name == elf)
    }
    pub(crate) fn tp(&self) -> (&'static str, &'static str) {
        match self.attach { AttachSpec::Tp(c, n) => (c, n), _ => panic!("{}", self.elf_name) }
    }
    pub(crate) fn lsm(&self) -> &'static str {
        match self.attach { AttachSpec::Lsm(h) => h, _ => panic!("{}", self.elf_name) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeCondition {
    RequiresNetworkPolicy,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeOutcome {
    Attached,
    NotRequested,
    Unavailable,
    Failed,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProbeStatus {
    pub outcome: ProbeOutcome,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModeUpdate {
    Attached,
    #[cfg(any(test, target_os = "linux"))]
    Unavailable(&'static str),
    #[cfg(any(test, target_os = "linux"))]
    Failed(&'static str),
    #[cfg(any(test, target_os = "linux"))]
    Unsupported(&'static str),
}

/// connect4 loader seam → [`ModeUpdate`] (Linux classification owner; also tested on macOS).
#[cfg(any(test, target_os = "linux"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Connect4Fault {
    MissingProgram,
    WrongProgramKind,
    LoadFailed,
    AttachFailed { kernel_lacks_point: bool },
}

#[cfg(any(test, target_os = "linux"))]
pub(crate) fn connect4_update(fault: Connect4Fault) -> ModeUpdate {
    match fault {
        Connect4Fault::MissingProgram => {
            ModeUpdate::Unavailable("connect4_hook missing from object")
        }
        Connect4Fault::WrongProgramKind => {
            ModeUpdate::Failed("connect4_hook is not a CgroupSockAddr program")
        }
        Connect4Fault::LoadFailed => ModeUpdate::Failed("connect4_hook load failed"),
        Connect4Fault::AttachFailed {
            kernel_lacks_point: true,
        } => ModeUpdate::Unsupported("kernel lacks cgroup/connect4 attach point"),
        Connect4Fault::AttachFailed {
            kernel_lacks_point: false,
        } => ModeUpdate::Failed("connect4_hook cgroup attach failed"),
    }
}

pub(crate) fn default_status(c: ProbeCondition) -> ProbeStatus {
    match c {
        ProbeCondition::RequiresNetworkPolicy => ProbeStatus {
            outcome: ProbeOutcome::NotRequested,
            reason: "network policy not requested",
        },
        ProbeCondition::Unsupported => ProbeStatus {
            outcome: ProbeOutcome::Unsupported,
            reason: "compiled; no attach owner",
        },
    }
}

/// Infallible mode-aware transitions. Declared-`Unsupported` rows keep their seed; policy-gated
/// rows take the update. Unknown probes are not updated (`record_mode` no-ops).
pub(crate) fn apply_mode_update(
    condition: ProbeCondition,
    current: ProbeStatus,
    update: ModeUpdate,
) -> ProbeStatus {
    match condition {
        ProbeCondition::Unsupported => current,
        ProbeCondition::RequiresNetworkPolicy => match update {
            ModeUpdate::Attached => ProbeStatus {
                outcome: ProbeOutcome::Attached,
                reason: "attached",
            },
            #[cfg(any(test, target_os = "linux"))]
            ModeUpdate::Unavailable(reason) => ProbeStatus {
                outcome: ProbeOutcome::Unavailable,
                reason,
            },
            #[cfg(any(test, target_os = "linux"))]
            ModeUpdate::Failed(reason) => ProbeStatus {
                outcome: ProbeOutcome::Failed,
                reason,
            },
            #[cfg(any(test, target_os = "linux"))]
            ModeUpdate::Unsupported(reason) => ProbeStatus {
                outcome: ProbeOutcome::Unsupported,
                reason,
            },
        },
    }
}

fn mode_row(name: &str) -> Option<&'static ProbeProgram> {
    PROBE_PROGRAMS
        .iter()
        .find(|p| p.surface_name == name && matches!(p.class, ProbeClass::Mode(_)))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeAttachment {
    attached: Vec<&'static str>,
    skipped: Vec<&'static str>,
    statuses: Vec<(&'static str, ProbeStatus)>,
}

impl Default for ProbeAttachment {
    fn default() -> Self {
        let mut a = Self {
            attached: Vec::new(),
            skipped: Vec::new(),
            statuses: Vec::new(),
        };
        for p in PROBE_PROGRAMS {
            if let ProbeClass::Mode(c) = p.class {
                a.set_status(p.surface_name, default_status(c));
            }
        }
        a
    }
}

impl ProbeAttachment {
    fn set_status(&mut self, probe: &'static str, status: ProbeStatus) {
        if let Some((_, s)) = self.statuses.iter_mut().find(|(n, _)| *n == probe) {
            *s = status;
        } else {
            self.statuses.push((probe, status));
        }
    }

    pub(crate) fn record_mode(&mut self, probe: &'static str, update: ModeUpdate) {
        let Some(row) = mode_row(probe) else {
            return;
        };
        let ProbeClass::Mode(condition) = row.class else {
            return;
        };
        let current = self
            .status(probe)
            .unwrap_or_else(|| default_status(condition));
        self.set_status(probe, apply_mode_update(condition, current, update));
    }

    pub fn attached(&mut self, probe: &'static str) {
        if !self.attached.contains(&probe) {
            self.attached.push(probe);
        }
        if mode_row(probe).is_some() {
            self.record_mode(probe, ModeUpdate::Attached);
        }
    }

    pub fn skipped(&mut self, probe: &'static str) {
        if !self.skipped.contains(&probe) {
            self.skipped.push(probe);
        }
    }

    pub fn reconcile(&mut self, expected: &[&'static str]) {
        for probe in expected {
            if !self.attached.contains(probe) {
                self.skipped(probe);
            }
        }
    }

    #[cfg(any(test, target_os = "linux"))]
    pub(crate) fn finalize_mode_aware(
        &self,
        network_policy_requested: bool,
    ) -> Result<(), &'static str> {
        if !network_policy_requested {
            return Ok(());
        }
        for p in PROBE_PROGRAMS.iter().filter(|p| {
            matches!(
                p.class,
                ProbeClass::Mode(ProbeCondition::RequiresNetworkPolicy)
            )
        }) {
            match self.outcome(p.surface_name) {
                Some(
                    ProbeOutcome::Attached
                    | ProbeOutcome::Unavailable
                    | ProbeOutcome::Failed
                    | ProbeOutcome::Unsupported,
                ) => {}
                _ => return Err("required probe requested but has no terminal status"),
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn attached_probes(&self) -> &[&'static str] {
        &self.attached
    }
    #[must_use]
    pub fn skipped_probes(&self) -> &[&'static str] {
        &self.skipped
    }
    #[must_use]
    pub fn status(&self, probe: &str) -> Option<ProbeStatus> {
        self.statuses
            .iter()
            .find(|(n, _)| *n == probe)
            .map(|(_, s)| *s)
    }
    #[must_use]
    pub fn outcome(&self, probe: &str) -> Option<ProbeOutcome> {
        self.status(probe).map(|s| s.outcome)
    }
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.skipped.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_complete_unchanged_for_always_probes() {
        let mut a = ProbeAttachment::default();
        for p in EXPECTED_PROBES {
            a.attached(p);
        }
        a.reconcile(EXPECTED_PROBES);
        assert!(a.is_complete());
        let mut b = ProbeAttachment::default();
        b.attached("sys_enter_openat");
        b.reconcile(EXPECTED_PROBES);
        assert!(b.skipped_probes().contains(&"lsm:file_open"));
        assert!(!b.is_complete());
    }

    #[test]
    fn no_policy_default_is_not_requested() {
        let a = ProbeAttachment::default();
        assert_eq!(
            a.outcome(EGRESS_PEER_PROBE),
            Some(ProbeOutcome::NotRequested)
        );
        assert!(a.finalize_mode_aware(false).is_ok());
    }

    #[test]
    fn missing_program_seam_is_unavailable() {
        let mut a = ProbeAttachment::default();
        a.record_mode(
            EGRESS_PEER_PROBE,
            connect4_update(Connect4Fault::MissingProgram),
        );
        assert_eq!(
            a.outcome(EGRESS_PEER_PROBE),
            Some(ProbeOutcome::Unavailable)
        );
    }

    #[test]
    fn wrong_kind_seam_is_failed() {
        let u = connect4_update(Connect4Fault::WrongProgramKind);
        assert!(matches!(u, ModeUpdate::Failed(_)));
        let mut a = ProbeAttachment::default();
        a.record_mode(EGRESS_PEER_PROBE, u);
        assert_eq!(a.outcome(EGRESS_PEER_PROBE), Some(ProbeOutcome::Failed));
    }

    #[test]
    fn load_or_attach_failure_seam_is_failed() {
        let mut a = ProbeAttachment::default();
        a.record_mode(
            EGRESS_PEER_PROBE,
            connect4_update(Connect4Fault::LoadFailed),
        );
        assert_eq!(a.outcome(EGRESS_PEER_PROBE), Some(ProbeOutcome::Failed));
        a.record_mode(
            EGRESS_PEER_PROBE,
            connect4_update(Connect4Fault::AttachFailed {
                kernel_lacks_point: false,
            }),
        );
        assert_eq!(a.outcome(EGRESS_PEER_PROBE), Some(ProbeOutcome::Failed));
    }

    #[test]
    fn unsupported_declared_surface_seeds_unsupported() {
        let a = ProbeAttachment::default();
        assert_eq!(
            a.outcome("sys_enter_sendto"),
            Some(ProbeOutcome::Unsupported)
        );
        assert_eq!(
            a.outcome("cgroup_sock_addr:connect6"),
            Some(ProbeOutcome::Unsupported)
        );
        assert!(matches!(
            connect4_update(Connect4Fault::AttachFailed {
                kernel_lacks_point: true
            }),
            ModeUpdate::Unsupported(_)
        ));
    }

    #[test]
    fn successful_attach_is_attached() {
        let mut a = ProbeAttachment::default();
        a.attached(EGRESS_PEER_PROBE);
        assert_eq!(a.outcome(EGRESS_PEER_PROBE), Some(ProbeOutcome::Attached));
        assert!(a.finalize_mode_aware(true).is_ok());
    }

    #[test]
    fn requested_without_terminal_finalize_errors() {
        assert_eq!(
            ProbeAttachment::default().finalize_mode_aware(true),
            Err("required probe requested but has no terminal status")
        );
    }

    #[test]
    fn unsupported_surface_ignores_attach_update() {
        let mut a = ProbeAttachment::default();
        a.record_mode("sys_enter_sendto", ModeUpdate::Attached);
        assert_eq!(
            a.outcome("sys_enter_sendto"),
            Some(ProbeOutcome::Unsupported)
        );
    }

    #[test]
    #[rustfmt::skip]
    fn program_table_is_the_inventory_owner() {
        assert_eq!(PROBE_PROGRAMS.len(), 11);
        assert_eq!(ALWAYS_N, 7);
        let always: Vec<_> = PROBE_PROGRAMS.iter().filter(|p| matches!(p.class, ProbeClass::Always)).map(|p| p.surface_name).collect();
        assert_eq!(EXPECTED_PROBES, always.as_slice());
        assert_eq!(EGRESS_PEER_PROBE, PROBE_PROGRAMS.iter().find(|p| p.elf_name == "connect4_hook").unwrap().surface_name);
        assert_eq!(PROBE_PROGRAMS.iter().map(|p| p.elf_name).collect::<Vec<_>>(), [
            "assay_monitor_openat", "assay_monitor_openat2", "assay_monitor_openat_exit",
            "assay_monitor_openat2_exit", "assay_monitor_connect", "assay_monitor_sendto",
            "assay_monitor_sendmsg", "assay_monitor_fork", "file_open_lsm", "connect4_hook",
            "connect6_hook",
        ]);
        for p in PROBE_PROGRAMS {
            match (p.elf_name, p.class, p.attach) {
                ("assay_monitor_sendto" | "assay_monitor_sendmsg" | "connect6_hook", ProbeClass::Mode(ProbeCondition::Unsupported), AttachSpec::None) => {
                    assert!(!EXPECTED_PROBES.contains(&p.surface_name));
                }
                ("file_open_lsm", ProbeClass::Always, AttachSpec::Lsm("file_open")) => {}
                ("connect4_hook", ProbeClass::Mode(ProbeCondition::RequiresNetworkPolicy), AttachSpec::Cgroup4) => {}
                (_, ProbeClass::Always, AttachSpec::Tp("syscalls", n)) if n == p.surface_name => {}
                other => panic!("{other:?}"),
            }
        }
    }
}
