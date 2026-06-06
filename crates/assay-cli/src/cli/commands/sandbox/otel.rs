//! Emit sandbox observations as OTel GenAI `execute_tool` spans (ADR-038 wiring).
//!
//! Maps the profiled observations (filesystem operations, executed programs,
//! containment degradations) to [`assay_core::otel::ToolObservation`] and writes
//! them via [`assay_core::otel::export_tool_spans_jsonl`] in the semconv-shaped
//! JSONL collector format, each carrying the claim-class outcome. This is the
//! sandbox side of the claimed-versus-actual surface.

use crate::profile::ProfileReport;
use assay_core::otel::{export_tool_spans_jsonl, OTelConfig, ToolObservation};
use std::path::Path;

pub(super) fn emit_tool_spans(
    report: &ProfileReport,
    run_id: &str,
    out: &Path,
) -> anyhow::Result<()> {
    let agg = &report.agg;
    let mut observations: Vec<ToolObservation> = Vec::new();

    // An observed effect under the sandbox is a supported claim (it happened and
    // was independently observed). Degradations weakened containment -> degraded.
    for (op, path, _backend) in &agg.fs {
        observations.push(ToolObservation {
            tool_name: format!("fs.{}", op.as_str()),
            claim_class_outcome: "supported".to_string(),
            subject: Some(path.clone()),
        });
    }
    for argv0 in agg.execs.keys() {
        observations.push(ToolObservation {
            tool_name: "exec".to_string(),
            claim_class_outcome: "supported".to_string(),
            subject: Some(argv0.clone()),
        });
    }
    for _degradation in &agg.sandbox_degradations {
        observations.push(ToolObservation {
            tool_name: "containment".to_string(),
            claim_class_outcome: "degraded".to_string(),
            subject: None,
        });
    }

    let cfg = OTelConfig {
        jsonl_path: Some(out.to_path_buf()),
        redact_prompts: false,
    };
    export_tool_spans_jsonl(&cfg, run_id, &observations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::events::{BackendHint, FsOp};
    use crate::profile::{ProfileAgg, ProfileConfig, ProfileReport};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn emit_tool_spans_writes_execute_tool_rows() {
        let mut execs = BTreeMap::new();
        execs.insert("sh".to_string(), 1);
        let agg = ProfileAgg {
            fs: vec![(FsOp::Write, "/tmp/x".to_string(), BackendHint::Landlock)],
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
        let out =
            std::env::temp_dir().join(format!("assay-sbx-otel-test-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&out);

        emit_tool_spans(&report, "sandbox_testrun", &out).expect("emit");

        let body = std::fs::read_to_string(&out).expect("read");
        let lines: Vec<&str> = body.lines().collect();
        // 1 fs op + 1 exec = 2 execute_tool spans.
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["attributes"]["gen_ai.operation.name"], "execute_tool");
        assert_eq!(first["attributes"]["gen_ai.tool.name"], "fs.write");
        assert_eq!(
            first["attributes"]["assay.claim_class.outcome"],
            "supported"
        );

        std::fs::remove_file(&out).ok();
    }
}
