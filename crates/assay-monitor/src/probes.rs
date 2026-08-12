//! Probe attachment + mode-aware inventory (`not_requested`≠`unavailable`≠`failed`≠`unsupported`).
//! Always probes: [`EXPECTED_PROBES`] + [`ProbeAttachment::reconcile`].
//! Mode-aware: `default_status` / `apply_mode_update`; Linux loader seams via `connect4_update`.

pub const EXPECTED_PROBES: &[&str] = &[
    "sys_enter_openat",
    "sys_enter_openat2",
    "sys_exit_openat",
    "sys_exit_openat2",
    "sys_enter_connect",
    "sys_enter_fork",
    "lsm:file_open",
];

pub const EGRESS_PEER_PROBE: &str = "cgroup_sock_addr:connect4";

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
pub(crate) struct ProbeSpec {
    pub name: &'static str,
    pub condition: ProbeCondition,
}

pub(crate) const MODE_AWARE_PROBES: &[ProbeSpec] = &[
    ProbeSpec {
        name: EGRESS_PEER_PROBE,
        condition: ProbeCondition::RequiresNetworkPolicy,
    },
    ProbeSpec {
        name: "cgroup_sock_addr:connect6",
        condition: ProbeCondition::Unsupported,
    },
    ProbeSpec {
        name: "sys_enter_sendto",
        condition: ProbeCondition::Unsupported,
    },
    ProbeSpec {
        name: "sys_enter_sendmsg",
        condition: ProbeCondition::Unsupported,
    },
];

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

fn mode_spec(name: &str) -> Option<&'static ProbeSpec> {
    MODE_AWARE_PROBES.iter().find(|s| s.name == name)
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
        for spec in MODE_AWARE_PROBES {
            a.set_status(spec.name, default_status(spec.condition));
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
        let Some(spec) = mode_spec(probe) else {
            return;
        };
        let current = self
            .status(probe)
            .unwrap_or_else(|| default_status(spec.condition));
        self.set_status(probe, apply_mode_update(spec.condition, current, update));
    }

    pub fn attached(&mut self, probe: &'static str) {
        if !self.attached.contains(&probe) {
            self.attached.push(probe);
        }
        if mode_spec(probe).is_some() {
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
        for spec in MODE_AWARE_PROBES
            .iter()
            .filter(|s| s.condition == ProbeCondition::RequiresNetworkPolicy)
        {
            match self.outcome(spec.name) {
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
}
