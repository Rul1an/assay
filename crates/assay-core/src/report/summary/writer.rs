use std::path::Path;

use super::types::Summary;

pub const SUMMARY_SCHEMA: &str = "assay.run_summary.v1";

/// Render the public summary artifact without expanding the public `Summary` Rust type.
///
/// The schema identity is added at the document boundary. Existing summary fields retain their
/// order and byte representation; render-safety for this artifact is tracked separately in #2168.
pub fn render_summary_json(summary: &Summary) -> anyhow::Result<String> {
    let mut value = serde_json::to_value(summary)?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("summary must serialize as a JSON object"))?;
    object.insert(
        "schema".to_string(),
        serde_json::Value::String(SUMMARY_SCHEMA.to_string()),
    );
    Ok(serde_json::to_string_pretty(&value)?)
}

/// Write summary.json to file.
pub fn write_summary(summary: &Summary, out: &Path) -> anyhow::Result<()> {
    let json = render_summary_json(summary)?;
    std::fs::write(out, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary() -> Summary {
        Summary::success("5.0.0", true)
    }

    #[test]
    fn renderer_adds_named_schema_without_changing_public_summary_type() {
        let rendered = render_summary_json(&summary()).expect("render summary");
        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse summary");

        assert_eq!(SUMMARY_SCHEMA, "assay.run_summary.v1");
        assert_eq!(value["schema"], SUMMARY_SCHEMA);
        assert_eq!(value["schema_version"], 1);
        assert_eq!(rendered.lines().nth(1), Some("  \"schema_version\": 1,"));
    }

    #[test]
    fn legacy_and_named_documents_deserialize_and_rerender_idempotently() {
        let legacy = serde_json::to_string(&summary()).expect("serialize legacy summary");
        let legacy_summary: Summary = serde_json::from_str(&legacy).expect("read legacy summary");
        let named = render_summary_json(&legacy_summary).expect("render named summary");
        let named_summary: Summary = serde_json::from_str(&named).expect("read named summary");
        let rerendered = render_summary_json(&named_summary).expect("rerender named summary");

        assert_eq!(named, rerendered);
    }
}
