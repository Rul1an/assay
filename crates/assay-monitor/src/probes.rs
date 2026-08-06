//! What the monitor actually attached, and what it did not.
//!
//! Cross-platform on purpose: the record is plain data, and only its emitter (`loader::LinuxMonitor`)
//! is Linux-gated — the same split `enforcement_health` uses.
//!
//! This exists because `MonitorLink` cannot answer the question. It records a program *kind*, and the
//! handles are held for RAII only, never inspected. Every attach site is guarded by
//! `if let Some(prog)` / `if let Ok(tp)`, so a program missing from the object, or failing to
//! convert, was skipped without a trace. That made an unattached probe and a probe that attached and
//! saw nothing indistinguishable — and a coverage descriptor built on that would report silence as
//! coverage, which is the one thing such a descriptor exists to prevent.

/// The probes `LinuxMonitor::attach` tries to install.
///
/// Names are eBPF program attach points, so an entry maps back to a source file in `assay-ebpf`
/// without guessing.
pub const EXPECTED_PROBES: &[&str] = &[
    "sys_enter_openat",
    "sys_enter_openat2",
    "sys_exit_openat",
    "sys_exit_openat2",
    "sys_enter_connect",
    "sched_process_fork",
    "lsm:file_open",
];

/// Which probes attached this run, and which did not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProbeAttachment {
    attached: Vec<&'static str>,
    skipped: Vec<&'static str>,
}

impl ProbeAttachment {
    pub fn attached(&mut self, probe: &'static str) {
        if !self.attached.contains(&probe) {
            self.attached.push(probe);
        }
    }

    pub fn skipped(&mut self, probe: &'static str) {
        if !self.skipped.contains(&probe) {
            self.skipped.push(probe);
        }
    }

    /// Reconcile against the probe set the loader tries to attach.
    ///
    /// Per-site recording alone is not enough, and enumerating the silent paths is fragile: a guard
    /// that skips leaves no trace at the site. Reconciling makes the record complete by
    /// construction — anything expected that did not attach is a skip, whatever the reason and
    /// whether or not that path was instrumented.
    pub fn reconcile(&mut self, expected: &[&'static str]) {
        for probe in expected {
            if !self.attached.contains(probe) {
                self.skipped(probe);
            }
        }
    }

    /// Probes that attached. An effect on a surface no entry here covers is not observable by this
    /// run, whatever the events say.
    #[must_use]
    pub fn attached_probes(&self) -> &[&'static str] {
        &self.attached
    }

    /// Probes the loader tried and did not get. A non-empty list is a blind spot this run can name,
    /// which is strictly better than one it cannot.
    #[must_use]
    pub fn skipped_probes(&self) -> &[&'static str] {
        &self.skipped
    }

    /// Whether every expected probe attached. `false` means at least one surface is unobserved, so
    /// an absence claim over it is unsupportable regardless of what the event stream shows.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.skipped.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_probe_that_never_attached_is_reported_as_skipped() {
        let mut attachment = ProbeAttachment::default();
        attachment.attached("sys_enter_openat");
        attachment.reconcile(EXPECTED_PROBES);

        assert_eq!(attachment.attached_probes(), ["sys_enter_openat"]);
        assert!(!attachment.is_complete());
        assert!(attachment.skipped_probes().contains(&"lsm:file_open"));
        assert!(
            !attachment.skipped_probes().contains(&"sys_enter_openat"),
            "an attached probe must never appear as skipped"
        );
    }

    #[test]
    fn reconciliation_catches_silent_paths_the_sites_never_recorded() {
        // The defect this guards: a guard skips without recording, so only reconciliation sees it.
        let mut attachment = ProbeAttachment::default();
        for probe in EXPECTED_PROBES
            .iter()
            .filter(|p| **p != "sched_process_fork")
        {
            attachment.attached(probe);
        }
        assert!(
            attachment.skipped_probes().is_empty(),
            "nothing recorded at the sites"
        );

        attachment.reconcile(EXPECTED_PROBES);
        assert_eq!(attachment.skipped_probes(), ["sched_process_fork"]);
    }

    #[test]
    fn a_fully_attached_run_is_complete() {
        let mut attachment = ProbeAttachment::default();
        for probe in EXPECTED_PROBES {
            attachment.attached(probe);
        }
        attachment.reconcile(EXPECTED_PROBES);
        assert!(attachment.is_complete());
        assert!(attachment.skipped_probes().is_empty());
    }

    #[test]
    fn recording_is_idempotent() {
        // attach() records at the site and reconcile() runs afterwards; a probe must not be listed
        // twice because both ran.
        let mut attachment = ProbeAttachment::default();
        attachment.attached("sys_enter_connect");
        attachment.attached("sys_enter_connect");
        attachment.skipped("lsm:file_open");
        attachment.skipped("lsm:file_open");
        attachment.reconcile(&["lsm:file_open"]);

        assert_eq!(attachment.attached_probes(), ["sys_enter_connect"]);
        assert_eq!(attachment.skipped_probes(), ["lsm:file_open"]);
    }

    #[test]
    fn the_monitor_wrapper_reports_blind_spots_where_nothing_can_attach() {
        // On a target with no eBPF the honest answer is "every surface unobserved", not an absent
        // record. A missing record reads as "not requested"; a full skip list reads as what it is.
        let attachment = crate::Monitor::probe_attachment_for_tests();
        assert!(!attachment.is_complete());
        assert!(attachment.attached_probes().is_empty() || cfg!(target_os = "linux"));
    }

    #[test]
    fn an_empty_run_reports_every_expected_probe_as_a_blind_spot() {
        let mut attachment = ProbeAttachment::default();
        attachment.reconcile(EXPECTED_PROBES);
        assert_eq!(attachment.skipped_probes().len(), EXPECTED_PROBES.len());
        assert!(!attachment.is_complete());
    }
}
