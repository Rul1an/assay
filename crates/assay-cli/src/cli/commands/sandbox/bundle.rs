//! Emit sandbox observations as a canonical evidence bundle.
//!
//! Builds CloudEvents-style `EvidenceEvent`s from the profiled observations
//! (filesystem operations, executed programs, containment degradations) and
//! writes them as a `.tar.gz` evidence bundle consumable by `assay evidence
//! lint` / `diff`. The run id is the deterministic profile run id; event
//! timestamps reflect emission time, matching the receipt-importer convention.

use crate::profile::events::FsOp;
use crate::profile::ProfileReport;
use anyhow::Context;
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use assay_evidence::{
    CodingAgentCoverage, CodingAgentCoverageState, CodingAgentDeclaredScope,
    CodingAgentEvidencePayload, CodingAgentNetworkPolicy, CodingAgentObservedEffects,
    CodingAgentSourceClass, CODING_AGENT_EVIDENCE_EVENT_TYPE,
};
use chrono::{DateTime, Utc};
use std::fs::File;
use std::path::Path;

const EVENT_SOURCE: &str = "urn:assay:sandbox";

pub(super) fn emit_bundle(
    report: &ProfileReport,
    command: &[String],
    run_id: &str,
    out: &Path,
) -> anyhow::Result<()> {
    let producer = ProducerMeta {
        name: "assay-cli".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        git: option_env!("ASSAY_GIT_SHA").map(|s| s.to_string()),
    };
    let emit_time = Utc::now();
    let agg = &report.agg;
    let mut events: Vec<EvidenceEvent> = Vec::new();

    // Summary event: command and aggregate counts.
    let summary = serde_json::json!({
        "command": command,
        "counters": agg.counters,
        "notes": agg.notes,
        "fs_count": agg.fs.len(),
        "exec_count": agg.execs.len(),
        "degradation_count": agg.sandbox_degradations.len(),
    });
    push_event(
        &mut events,
        "assay.sandbox.summary",
        run_id,
        summary,
        None,
        emit_time,
        &producer,
    );

    // Filesystem observations (deterministic order: agg.fs is collected in order).
    for (op, path, backend) in &agg.fs {
        let payload = serde_json::json!({
            "op": op.as_str(),
            "path": path,
            "backend": backend.as_str(),
        });
        push_event(
            &mut events,
            "assay.sandbox.fs",
            run_id,
            payload,
            Some(path.clone()),
            emit_time,
            &producer,
        );
    }

    // Executed programs (BTreeMap iterates in sorted, deterministic order).
    for (argv0, hits) in &agg.execs {
        let payload = serde_json::json!({ "argv0": argv0, "hits": hits });
        push_event(
            &mut events,
            "assay.sandbox.exec",
            run_id,
            payload,
            Some(argv0.clone()),
            emit_time,
            &producer,
        );
    }

    // Containment degradations that weakened enforcement while execution continued.
    for degradation in &agg.sandbox_degradations {
        let payload =
            serde_json::to_value(degradation).context("serialize sandbox degradation payload")?;
        push_event(
            &mut events,
            "assay.sandbox.degraded",
            run_id,
            payload,
            None,
            emit_time,
            &producer,
        );
    }

    // The coding-agent evidence pack (ADR-035's convergence slice). The sandbox is the producer:
    // Landlock observes at a boundary the agent does not control, so `boundary_observed` is the
    // honest source class — but only for the surfaces it actually watches.
    let pack = coding_agent_pack(report, command);
    push_event(
        &mut events,
        CODING_AGENT_EVIDENCE_EVENT_TYPE,
        run_id,
        serde_json::to_value(&pack).context("serialize coding-agent evidence pack")?,
        None,
        emit_time,
        &producer,
    );

    let file = File::create(out)
        .with_context(|| format!("create evidence bundle at {}", out.display()))?;
    let mut writer = BundleWriter::new(file).with_producer(producer);
    for event in events {
        writer.add_event(event);
    }
    writer.finish().context("write sandbox evidence bundle")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn push_event(
    events: &mut Vec<EvidenceEvent>,
    type_: &str,
    run_id: &str,
    payload: serde_json::Value,
    subject: Option<String>,
    time: DateTime<Utc>,
    producer: &ProducerMeta,
) {
    let seq = events.len() as u64;
    let mut event = EvidenceEvent::new(type_, EVENT_SOURCE, run_id, seq, payload)
        .with_time(time)
        .with_producer(producer);
    if let Some(subject) = subject {
        event = event.with_subject(subject);
    }
    events.push(event);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::events::{BackendHint, FsOp};
    use crate::profile::{ProfileAgg, ProfileConfig, ProfileReport};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn emit_bundle_produces_a_verifiable_bundle() {
        let mut execs = BTreeMap::new();
        execs.insert("sh".to_string(), 2);
        let agg = ProfileAgg {
            fs: vec![(
                FsOp::Write,
                "/tmp/out.txt".to_string(),
                BackendHint::Landlock,
            )],
            execs,
            ..Default::default()
        };
        let report = ProfileReport {
            version: 1,
            config: ProfileConfig {
                cwd: PathBuf::from("/tmp"),
                home: None,
                assay_tmp: None,
            },
            agg,
        };
        let command = vec!["echo".to_string(), "hi".to_string()];
        // `tempfile::tempdir()` rather than `std::env::temp_dir()`: it is what the rest of this
        // crate's tests use, it cleans up on drop instead of relying on a PID-suffixed name, and it
        // keeps `TMPDIR` out of the path expression that reaches `File::open`.
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("assay-sbx-bundle-test.tar.gz");

        emit_bundle(&report, &command, "sandbox_testrun", &out).expect("emit bundle");

        let file = File::open(&out).expect("open bundle");
        let result = assay_evidence::bundle::verify_bundle(file).expect("verify bundle");
        // summary + 1 fs op + 1 exec entry + the coding-agent evidence pack.
        assert_eq!(result.event_count, 4);

        // Name the pack rather than only counting, so a future event addition cannot quietly stand
        // in for it. The pack is what carries source class and coverage; the rest are observations.
        // Read the events back inline rather than through a helper taking a path: the helper made
        // the temp path cross a function boundary, which taint analysis reads as an uncontrolled
        // path expression. Same assertion, one less indirection.
        let events =
            assay_evidence::bundle::BundleReader::open(File::open(&out).expect("reopen bundle"))
                .expect("open bundle reader")
                .events_vec()
                .expect("read bundle events");
        assert!(
            events
                .iter()
                .any(|e| e.type_ == assay_evidence::CODING_AGENT_EVIDENCE_EVENT_TYPE),
            "bundle must carry the coding-agent evidence pack, got {:?}",
            events.iter().map(|e| e.type_.as_str()).collect::<Vec<_>>()
        );

        // No explicit cleanup: `dir` removes the tree on drop.
    }
}

/// Build the coding-agent evidence pack from what the sandbox actually observed.
///
/// The load-bearing part is the coverage, not the effects. A Landlock filesystem sandbox watches
/// file operations and execs; it does **not** watch network egress or MCP tool calls. Emitting an
/// empty `network_attempts` with `coverage.network = absent` says "nobody looked", which the claim
/// gate reads as unsupportable — where an empty list with `observed` would have claimed the agent
/// made no network attempt. Those are different facts and the pack must not conflate them.
fn coding_agent_pack(report: &ProfileReport, command: &[String]) -> CodingAgentEvidencePayload {
    let agg = &report.agg;

    let mut files_changed: Vec<String> = agg
        .fs
        .iter()
        .filter(|(op, _, _)| matches!(op, FsOp::Write))
        .map(|(_, path, _)| path.clone())
        .collect();
    files_changed.sort();
    files_changed.dedup();

    let commands_executed: Vec<String> = agg.execs.keys().cloned().collect();

    // A degradation is containment that weakened while execution continued, so what the sandbox saw
    // after it is not a complete account of the surfaces it governs.
    let watched = if agg.sandbox_degradations.is_empty() {
        CodingAgentCoverageState::Observed
    } else {
        CodingAgentCoverageState::Partial
    };

    CodingAgentEvidencePayload::new(
        CodingAgentDeclaredScope {
            // The sandbox declares its scope as a policy, which is not carried into the profile
            // report; an empty allowlist here is "not declared in this artifact", and `authorized`
            // stays false so no consumer reads it as an approved scope.
            allowed_files: Vec::new(),
            allowed_commands: command.first().cloned().into_iter().collect(),
            // Landlock network enforcement is a separate opt-in and is not observable from the
            // profile report, so the pack does not claim a network policy either way.
            network: CodingAgentNetworkPolicy::Allowed,
            allowed_mcp_tools: Vec::new(),
            expected_test_command: None,
            authorized: false,
        },
        CodingAgentObservedEffects {
            files_changed,
            commands_executed,
            // Empty because unobserved, never because none occurred — see `coverage` below.
            network_attempts: Vec::new(),
            mcp_tool_calls: Vec::new(),
            test_observed: false,
        },
        CodingAgentCoverage {
            files: watched,
            commands: watched,
            network: CodingAgentCoverageState::Absent,
            mcp_tools: CodingAgentCoverageState::Absent,
            test: CodingAgentCoverageState::Absent,
        },
        CodingAgentSourceClass::BoundaryObserved,
    )
}

#[cfg(test)]
mod coding_agent_pack_tests {
    use super::*;
    use crate::profile::events::BackendHint;
    use crate::profile::{ProfileAgg, ProfileConfig};
    use assay_evidence::types::{
        PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode,
        SandboxDegradationReasonCode,
    };
    use assay_evidence::{
        coding_agent_claim_decision, CodingAgentClaimKind, CodingAgentGateDecision,
    };
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn report(fs: Vec<(FsOp, String, BackendHint)>, degraded: bool) -> ProfileReport {
        let mut execs = BTreeMap::new();
        execs.insert("/usr/bin/git".to_string(), 2u64);
        ProfileReport {
            version: 1,
            config: ProfileConfig {
                cwd: PathBuf::from("/repo"),
                home: None,
                assay_tmp: None,
            },
            agg: ProfileAgg {
                counters: BTreeMap::new(),
                env_provided: BTreeMap::new(),
                execs,
                fs,
                notes: Vec::new(),
                sandbox_degradations: if degraded {
                    // A real degradation: Landlock fell back to audit, so containment weakened
                    // while execution continued.
                    vec![PayloadSandboxDegraded {
                        reason_code: SandboxDegradationReasonCode::BackendUnavailable,
                        degradation_mode: SandboxDegradationMode::AuditFallback,
                        component: SandboxDegradationComponent::Landlock,
                        detail: None,
                    }]
                } else {
                    Vec::new()
                },
            },
        }
    }

    #[test]
    fn only_writes_become_files_changed() {
        let pack = coding_agent_pack(
            &report(
                vec![
                    (FsOp::Read, "/repo/README.md".into(), BackendHint::Injected),
                    (
                        FsOp::Write,
                        "/repo/src/lib.rs".into(),
                        BackendHint::Injected,
                    ),
                    (
                        FsOp::Write,
                        "/repo/src/lib.rs".into(),
                        BackendHint::Injected,
                    ),
                ],
                false,
            ),
            &["git".to_string()],
        );
        assert_eq!(pack.observed_effects.files_changed, ["/repo/src/lib.rs"]);
        assert_eq!(pack.observed_effects.commands_executed, ["/usr/bin/git"]);
    }

    #[test]
    fn unwatched_surfaces_are_absent_not_empty_observations() {
        // The defect this guards: an empty `network_attempts` with coverage `observed` would claim
        // the agent made no network attempt. A Landlock filesystem sandbox never looked.
        let pack = coding_agent_pack(&report(Vec::new(), false), &["git".to_string()]);
        assert!(pack.observed_effects.network_attempts.is_empty());
        assert_eq!(pack.coverage.network, CodingAgentCoverageState::Absent);
        assert_eq!(pack.coverage.mcp_tools, CodingAgentCoverageState::Absent);
        assert_eq!(pack.source_class, CodingAgentSourceClass::BoundaryObserved);
    }

    #[test]
    fn an_absence_claim_about_the_network_is_blocked_by_the_gate() {
        // The end-to-end point of the pack: the claim gate must refuse to conclude "no egress"
        // from a run that never watched egress, however strong the source class is.
        let pack = coding_agent_pack(&report(Vec::new(), false), &["git".to_string()]);
        let decision = coding_agent_claim_decision(
            pack.source_class,
            pack.coverage.network,
            CodingAgentClaimKind::BoundedNegative,
        );
        assert_eq!(decision.decision, CodingAgentGateDecision::Blocked);

        // ...while the surfaces it did watch still support an absence claim.
        let files = coding_agent_claim_decision(
            pack.source_class,
            pack.coverage.files,
            CodingAgentClaimKind::BoundedNegative,
        );
        assert_eq!(files.decision, CodingAgentGateDecision::Allowed);
    }

    #[test]
    fn a_containment_degradation_downgrades_the_watched_surfaces() {
        let pack = coding_agent_pack(&report(Vec::new(), true), &["git".to_string()]);
        assert_eq!(pack.coverage.files, CodingAgentCoverageState::Partial);

        // Partial still supports saying what was seen, and stops supporting absence.
        assert_eq!(
            coding_agent_claim_decision(
                pack.source_class,
                pack.coverage.files,
                CodingAgentClaimKind::PositiveExistence,
            )
            .decision,
            CodingAgentGateDecision::Allowed
        );
        assert_eq!(
            coding_agent_claim_decision(
                pack.source_class,
                pack.coverage.files,
                CodingAgentClaimKind::BoundedNegative,
            )
            .decision,
            CodingAgentGateDecision::Blocked
        );
    }

    #[test]
    fn the_pack_declares_no_authorized_scope_it_cannot_see() {
        let pack = coding_agent_pack(&report(Vec::new(), false), &["git".to_string()]);
        assert!(
            !pack.declared_scope.authorized,
            "an undeclared scope is never approved"
        );
        assert!(pack.declared_scope.allowed_files.is_empty());
    }
}
