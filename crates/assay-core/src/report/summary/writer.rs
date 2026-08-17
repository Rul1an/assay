use std::path::Path;

use crate::render_safety::{render_details_safe, Sink};

use super::types::Summary;

pub const SUMMARY_SCHEMA: &str = "assay.run_summary.v1";

/// Render the public summary artifact without expanding the public `Summary` Rust type.
///
/// The schema identity is added at the document boundary. Untrusted failure text is rendered safe
/// for the JSON record sink while Assay-owned fields retain their order and byte representation.
pub fn render_summary_json(summary: &Summary) -> anyhow::Result<String> {
    let value = serde_json::to_value(summary)?;
    let mut value = render_details_safe(Sink::Json, &value, usize::MAX);
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
        assert_eq!(
            rendered.lines().nth(1),
            Some("  \"schema_version\": 1,"),
            "serde_json preserve_order must retain the historical first summary key"
        );
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

    #[test]
    fn renderer_sanitizes_untrusted_failure_text_without_changing_owned_fields() {
        let secret = format!("ghp_{}", "S".repeat(36));
        let message = format!(
            "provider said \u{1b}[2J{secret}\u{7} {}",
            "payload".repeat(80)
        );
        let recovery_path = "\u{1b}[31m/tmp/alice@example.com\u{1b}[0m";
        let next_step = format!(
            "Run argv: {}",
            serde_json::json!(["assay", "doctor", format!("--config={recovery_path}")])
        );
        let summary = Summary::failure(3, "E_PROVIDER", &message, &next_step, "5.2.0", true);
        let mut expected_owned = serde_json::to_value(&summary).expect("serialize summary");

        let rendered = render_summary_json(&summary).expect("render summary");
        let mut actual: serde_json::Value = serde_json::from_str(&rendered).expect("parse summary");
        let rendered_message = actual["message"].as_str().expect("message string");
        let rendered_next_step = actual["next_step"].as_str().expect("next_step string");

        assert!(!rendered_message.contains(&secret), "message leaked secret");
        assert!(rendered_message.contains("<redacted:github-token>"));
        assert!(
            rendered_message.len() > 500,
            "record sink must not truncate the message"
        );
        assert!(!rendered_message.contains('\u{1b}'));
        assert!(!rendered_message.contains('\u{7}'));
        assert_eq!(
            rendered_next_step, &next_step,
            "executable recovery contract changed"
        );
        let recovery: Vec<String> = serde_json::from_str(
            rendered_next_step
                .strip_prefix("Run argv: ")
                .expect("recovery prefix"),
        )
        .expect("recovery argv remains parseable");
        assert_eq!(recovery[2], format!("--config={recovery_path}"));

        let expected_object = expected_owned.as_object_mut().expect("summary object");
        expected_object.remove("message");
        let actual_object = actual.as_object_mut().expect("rendered summary object");
        actual_object.remove("schema");
        actual_object.remove("message");
        assert_eq!(actual, expected_owned, "Assay-owned fields changed");
    }
}
