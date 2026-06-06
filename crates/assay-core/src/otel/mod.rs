use crate::model::TestResultRow;

pub mod genai;
pub mod metrics;
pub mod redaction;
pub mod semconv;

#[derive(Debug, Clone, Default)]
pub struct OTelConfig {
    pub jsonl_path: Option<std::path::PathBuf>,
    pub redact_prompts: bool,
}

pub fn export_jsonl(
    cfg: &OTelConfig,
    _suite: &str,
    results: &[TestResultRow],
) -> anyhow::Result<()> {
    let Some(path) = &cfg.jsonl_path else {
        return Ok(());
    };
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    for r in results {
        // GenAI Semantic Conventions (simplified for MVP)
        // https://opentelemetry.io/docs/specs/semconv/gen-ai/
        let row = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "attributes": {
                "gen_ai.system": "assay",
                "gen_ai.request.model": "unknown", // can be enriched if we track it better
                "gen_ai.response.completion_tokens": 0, // placeholder
                "assay.test_id": r.test_id,
                "assay.status": format!("{:?}", r.status),
                "assay.score": r.score,
                "assay.cached": r.cached,
                "assay.duration_ms": r.duration_ms,
            }
        });

        // Use details/meta if available to populate standard fields
        // checking details logic would go here

        use std::io::Write;
        writeln!(f, "{}", row)?;
    }
    Ok(())
}

/// A single observed tool effect to emit as an OTel GenAI `execute_tool` span,
/// carrying the Assay claim-class outcome (the claimed-versus-actual surface).
#[derive(Debug, Clone)]
pub struct ToolObservation {
    /// Tool / effect name (e.g. an MCP tool name or a sandbox effect kind).
    pub tool_name: String,
    /// Assay claim-class outcome: `supported` | `degraded` | `blocked` | `not_evaluable`.
    pub claim_class_outcome: String,
    /// Optional subject (e.g. a path or resource).
    pub subject: Option<String>,
}

/// Emit observed tool effects as OTel GenAI `execute_tool` spans in the
/// semconv-shaped JSONL collector format (the same pattern as [`export_jsonl`]),
/// each carrying the Assay claim-class outcome as an attribute. Pinned to GenAI
/// semconv 1.28.0. A no-op unless `cfg.jsonl_path` is set.
///
/// This is the emit side of the claimed-versus-actual surface: a downstream OTel
/// collector ingests these spans alongside the agent's self-reported spans, so a
/// consumer can compare declared behavior against the independently observed
/// effect and the claim it actually supports.
pub fn export_tool_spans_jsonl(
    cfg: &OTelConfig,
    run: &str,
    observations: &[ToolObservation],
) -> anyhow::Result<()> {
    let Some(path) = &cfg.jsonl_path else {
        return Ok(());
    };
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    use std::io::Write;
    for (seq, obs) in observations.iter().enumerate() {
        // OTel GenAI execute-tool span (semconv 1.28.0), plus the assay claim-class
        // outcome as a vendor extension attribute.
        let row = serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "name": "execute_tool",
            "attributes": {
                "gen_ai.system": "assay",
                "gen_ai.operation.name": "execute_tool",
                "gen_ai.tool.name": obs.tool_name,
                "assay.claim_class.outcome": obs.claim_class_outcome,
                "assay.run": run,
                "assay.seq": seq,
                "assay.subject": obs.subject,
            },
        });
        writeln!(f, "{}", row)?;
    }
    Ok(())
}

#[cfg(test)]
mod tool_span_tests {
    use super::*;

    #[test]
    fn export_tool_spans_writes_execute_tool_rows_with_claim_class() {
        let path = std::env::temp_dir().join(format!(
            "assay-otel-tool-spans-{}.jsonl",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let cfg = OTelConfig {
            jsonl_path: Some(path.clone()),
            redact_prompts: false,
        };
        let observations = vec![
            ToolObservation {
                tool_name: "fs.write".into(),
                claim_class_outcome: "supported".into(),
                subject: Some("/tmp/out.txt".into()),
            },
            ToolObservation {
                tool_name: "net.connect".into(),
                claim_class_outcome: "blocked".into(),
                subject: None,
            },
        ];

        export_tool_spans_jsonl(&cfg, "sandbox_testrun", &observations).expect("export");

        let body = std::fs::read_to_string(&path).expect("read jsonl");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 2);
        let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(first["name"], "execute_tool");
        assert_eq!(first["attributes"]["gen_ai.operation.name"], "execute_tool");
        assert_eq!(first["attributes"]["gen_ai.tool.name"], "fs.write");
        assert_eq!(
            first["attributes"]["assay.claim_class.outcome"],
            "supported"
        );
        let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
        assert_eq!(second["attributes"]["assay.claim_class.outcome"], "blocked");

        std::fs::remove_file(&path).ok();
    }
}
