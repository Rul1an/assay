use crate::model::TestResultRow;

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
