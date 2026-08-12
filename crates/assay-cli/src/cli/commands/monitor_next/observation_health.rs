//! `assay.runner.observation_health.v0` — the observation-truth artifact, produced by `assay monitor`.
//!
//! Deliberately a SEPARATE carrier from `enforcement_health`, exactly as that module's header says:
//! it answers "was enforcement active, and did it block?", this answers "how complete was
//! observation?". A run can have complete observation and absent enforcement, or the reverse.
//!
//! Until now nothing in the workspace produced this schema — `assay project-otel` could consume one
//! and `assay-runner-schema` defined it, but no command emitted it. That gap mattered: under the
//! runner's own coverage-descriptor gate a missing descriptor blocks every claim kind, so a monitor
//! run could not support a bounded claim by this project's own standard.
//!
//! ## Coverage is derived from ATTACHMENT, not from emitted events
//!
//! `assay-runner-core`'s archive path derives network coverage from event counts
//! (`tracepoints.connect_emitted > 0`). It has to: the archive is assembled from stats, and
//! attachment is not in them. But emission and attachment answer different questions. Emission
//! proves a probe attached; **zero emission proves nothing at all** — a connect probe that attached
//! and correctly saw no connects is indistinguishable, in the counts, from one that never attached.
//!
//! The monitor has the better input. `ProbeAttachment` records what actually attached, so this
//! module derives coverage from that. The difference is not cosmetic: an attached-but-silent network
//! probe is `connect_only` here and would be `absent` from counts alone, and `absent` coverage
//! blocks an absence claim that the run can in fact support.
//!
//! Where the two could disagree the emission-based answer is the conservative one — it understates
//! coverage rather than overstating it — so this is a correctness improvement in the safe direction,
//! not a loosening.

use assay_monitor::probes::{ProbeAttachment, ProbeOutcome, EGRESS_PEER_PROBE};
use assay_runner_schema::{
    CgroupCorrelationStatus, KernelLayerStatus, NetworkEndpointClaimScope,
    NetworkProtocolCoverageStatus, ObservationHealth, PolicyLayerStatus, SdkLayerStatus,
};
use std::path::Path;

/// Tracepoint that must not earn peer coverage (tests only).
#[cfg(test)]
const CONNECT_PROBE: &str = "sys_enter_connect";

/// A deterministic run id over what the run observed, matching the sandbox convention
/// (`sandbox_<digest-prefix>`): the same observation produces the same id, and nothing is invented
/// from a clock or a random source that a reader could not recompute.
pub(crate) fn run_id(
    attachment: &ProbeAttachment,
    ringbuf_drops: u64,
    policy_declared: bool,
) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    for probe in attachment.attached_probes() {
        hasher.update(b"attached\0");
        hasher.update(probe.as_bytes());
    }
    for probe in attachment.skipped_probes() {
        hasher.update(b"skipped\0");
        hasher.update(probe.as_bytes());
    }
    hasher.update(ringbuf_drops.to_string().as_bytes());
    hasher.update(if policy_declared {
        &b"policy"[..]
    } else {
        &b"nopolicy"[..]
    });
    format!("monitor_{}", &hex::encode(hasher.finalize())[..16])
}

/// Build the artifact from what this run actually knows.
///
/// `ringbuf_drops` is total lost records, which the schema's own rules turn into a degraded kernel
/// layer — a run that lost records did not observe completely, whatever it did emit.
pub(crate) fn build(
    run_id: &str,
    attachment: &ProbeAttachment,
    ringbuf_drops: u64,
    policy_declared: bool,
) -> ObservationHealth {
    // Inventory outcome for the cgroup peer-source (not the connect tracepoint, not event counts).
    let network_protocol_coverage =
        if attachment.outcome(EGRESS_PEER_PROBE) == Some(ProbeOutcome::Attached) {
            NetworkProtocolCoverageStatus::ConnectOnly
        } else {
            NetworkProtocolCoverageStatus::Absent
        };

    let mut health = ObservationHealth::new(run_id, "linux")
        .with_policy_layer(if policy_declared {
            PolicyLayerStatus::Present
        } else {
            PolicyLayerStatus::Absent
        })
        // The monitor has no SDK shim; nothing self-reports into this run.
        .with_sdk_layer(SdkLayerStatus::Absent)
        .with_cgroup_correlation(CgroupCorrelationStatus::Clean);

    // `ObservationHealth::new` starts at `Absent` and its rules only ever downgrade — there is no
    // promotion path, so the kernel layer has to be derived here. This mirrors the runner's
    // `kernel_layer_for`: nothing lost and a clean correlation is complete, lost records are
    // partial, and nothing attached is absent regardless of counts.
    health.kernel_layer = if attachment.attached_probes().is_empty() {
        KernelLayerStatus::Absent
    } else if ringbuf_drops > 0 {
        KernelLayerStatus::PartialRingbufDrops
    } else {
        KernelLayerStatus::Complete
    };
    // Applied after the layer so the schema's own rules can still downgrade what was just set.
    health = health.with_ringbuf_drops(ringbuf_drops);

    health.network_protocol_coverage = network_protocol_coverage;
    health.network_endpoint_claim_scope = match network_protocol_coverage {
        // Connect-time endpoints are not an exhaustive peer set — the coverage descriptor for this
        // completeness says so itself, naming QUIC/datagram peer changes after connect as a blind
        // spot. Diagnostic, never a peer-set claim.
        NetworkProtocolCoverageStatus::ConnectOnly => NetworkEndpointClaimScope::DiagnosticOnly,
        _ => NetworkEndpointClaimScope::NotApplicable,
    };

    // Name every surface nobody watched. A reader should not have to infer a blind spot from an
    // empty event stream, which is the whole reason this artifact exists.
    for probe in attachment.skipped_probes() {
        health
            .notes
            .push(format!("probe not attached: {probe} (surface unobserved)"));
    }

    health
}

/// Write the artifact. Returns false if it was requested and could not be written.
pub(crate) fn write_to(health: &ObservationHealth, path: &Path) -> bool {
    let json = match serde_json::to_string_pretty(health) {
        Ok(json) => json,
        Err(err) => {
            eprintln!("Failed to serialize observation_health artifact: {err}");
            return false;
        }
    };
    if let Err(err) = std::fs::write(path, format!("{json}\n")) {
        eprintln!(
            "Failed to write observation_health artifact to {}: {err}",
            path.display()
        );
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_runner_schema::KernelLayerStatus;

    fn attachment(attached: &[&'static str], skipped: &[&'static str]) -> ProbeAttachment {
        let mut a = ProbeAttachment::default();
        for probe in attached {
            a.attached(probe);
        }
        for probe in skipped {
            a.skipped(probe);
        }
        a
    }

    #[test]
    fn the_connect_tracepoint_alone_does_not_earn_network_coverage() {
        // Regression guard for a defect found only by a live run. The tracepoint `sys_enter_connect`
        // is always attached, but it is NOT the peer source: peers come from the cgroup hook, which
        // attaches only when a policy is loaded. Keying coverage on the tracepoint made a run report
        // `connect_only` while its peer set was structurally empty — and a consumer reads that pair
        // as "watched and saw nothing", which is a false refutation.
        let health = build("run1", &attachment(&[CONNECT_PROBE], &[]), 0, false);
        assert_eq!(
            health.network_protocol_coverage,
            NetworkProtocolCoverageStatus::Absent,
            "the tracepoint sees connects but supplies no peers, so it cannot ground an absence claim"
        );
    }

    #[test]
    fn an_attached_but_silent_connect_probe_still_reports_coverage() {
        // The defect this artifact exists to avoid. Deriving from emitted counts, a run where the
        // connect probe attached and correctly saw nothing reports `absent` coverage — and absent
        // coverage blocks an absence claim the run can actually support.
        let health = build("run1", &attachment(&[EGRESS_PEER_PROBE], &[]), 0, false);
        assert_eq!(
            health.network_protocol_coverage,
            NetworkProtocolCoverageStatus::ConnectOnly
        );
        assert_eq!(
            health.network_endpoint_claim_scope,
            NetworkEndpointClaimScope::DiagnosticOnly
        );
    }

    #[test]
    fn an_unattached_connect_probe_reports_absent_coverage() {
        let health = build("run1", &attachment(&[], &[CONNECT_PROBE]), 0, false);
        assert_eq!(
            health.network_protocol_coverage,
            NetworkProtocolCoverageStatus::Absent
        );
        assert_eq!(
            health.network_endpoint_claim_scope,
            NetworkEndpointClaimScope::NotApplicable
        );
    }

    #[test]
    fn every_unattached_probe_is_named_in_the_notes() {
        let health = build(
            "run1",
            &attachment(&["lsm:file_open"], &["sys_enter_fork", CONNECT_PROBE]),
            0,
            false,
        );
        assert!(health
            .notes
            .iter()
            .any(|n| n.contains("sys_enter_fork") && n.contains("unobserved")));
        assert!(health.notes.iter().any(|n| n.contains(CONNECT_PROBE)));
    }

    #[test]
    fn a_run_where_nothing_attached_has_an_absent_kernel_layer() {
        // No probe attached is not "clean with no events" — it is no observation at all.
        let health = build("run1", &attachment(&[], &[CONNECT_PROBE]), 0, false);
        assert_eq!(health.kernel_layer, KernelLayerStatus::Absent);
    }

    #[test]
    fn lost_records_degrade_the_kernel_layer() {
        let clean = build("run1", &attachment(&[CONNECT_PROBE], &[]), 0, false);
        assert_eq!(clean.kernel_layer, KernelLayerStatus::Complete);

        let lossy = build("run1", &attachment(&[CONNECT_PROBE], &[]), 7, false);
        assert_eq!(lossy.kernel_layer, KernelLayerStatus::PartialRingbufDrops);
        assert_eq!(lossy.ringbuf_drops, 7);
    }

    #[test]
    fn a_declared_policy_is_the_only_thing_that_makes_the_policy_layer_present() {
        assert_eq!(
            build("run1", &attachment(&[], &[]), 0, false).policy_layer,
            PolicyLayerStatus::Absent
        );
        assert_eq!(
            build("run1", &attachment(&[], &[]), 0, true).policy_layer,
            PolicyLayerStatus::Present
        );
    }

    #[test]
    fn the_run_id_is_deterministic_and_distinguishes_observations() {
        let a = attachment(&[CONNECT_PROBE], &["sys_enter_fork"]);
        assert_eq!(
            run_id(&a, 0, false),
            run_id(&a, 0, false),
            "same run, same id"
        );

        // A different blind spot is a different observation and must not share an id.
        let b = attachment(&[CONNECT_PROBE], &[]);
        assert_ne!(run_id(&a, 0, false), run_id(&b, 0, false));
        assert_ne!(
            run_id(&a, 0, false),
            run_id(&a, 1, false),
            "lost records change the run"
        );
        assert!(run_id(&a, 0, false).starts_with("monitor_"));
    }

    #[test]
    fn the_artifact_round_trips() {
        let health = build(
            "run1",
            &attachment(&[CONNECT_PROBE], &["sys_enter_fork"]),
            2,
            true,
        );
        let json = serde_json::to_string(&health).expect("serialize");
        let back: ObservationHealth = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, health);
        assert_eq!(back.schema, "assay.runner.observation_health.v0");
    }
}
