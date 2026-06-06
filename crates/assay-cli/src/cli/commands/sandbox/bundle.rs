//! Emit sandbox observations as a canonical evidence bundle.
//!
//! Builds CloudEvents-style `EvidenceEvent`s from the profiled observations
//! (filesystem operations, executed programs, containment degradations) and
//! writes them as a `.tar.gz` evidence bundle consumable by `assay evidence
//! lint` / `diff`. The run id is the deterministic profile run id; event
//! timestamps reflect emission time, matching the receipt-importer convention.

use crate::profile::ProfileReport;
use anyhow::Context;
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
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
        let out = std::env::temp_dir().join(format!(
            "assay-sbx-bundle-test-{}.tar.gz",
            std::process::id()
        ));

        emit_bundle(&report, &command, "sandbox_testrun", &out).expect("emit bundle");

        let file = File::open(&out).expect("open bundle");
        let result = assay_evidence::bundle::verify_bundle(file).expect("verify bundle");
        // summary + 1 fs op + 1 exec entry = 3 events.
        assert_eq!(result.event_count, 3);

        std::fs::remove_file(&out).ok();
    }
}
