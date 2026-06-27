use anyhow::{Context, Result};
use assay_core::mcp::policy::McpPolicy;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::Path;

fn is_explicit_tool_name(pattern: &str) -> bool {
    let trimmed = pattern.trim();
    !trimmed.is_empty() && !trimmed.contains('*')
}

pub(super) fn collect_declared_tools(policy: &McpPolicy) -> Vec<String> {
    let mut declared = BTreeSet::new();

    if let Some(allow) = &policy.tools.allow {
        for tool in allow {
            if is_explicit_tool_name(tool) {
                declared.insert(tool.trim().to_string());
            }
        }
    }

    if let Some(deny) = &policy.tools.deny {
        for tool in deny {
            if is_explicit_tool_name(tool) {
                declared.insert(tool.trim().to_string());
            }
        }
    }

    for tool in policy.schemas.keys() {
        if tool != "$defs" && is_explicit_tool_name(tool) {
            declared.insert(tool.trim().to_string());
        }
    }

    for constraint in &policy.constraints {
        if is_explicit_tool_name(&constraint.tool) {
            declared.insert(constraint.tool.trim().to_string());
        }
    }

    for tool in policy.tool_pins.keys() {
        if is_explicit_tool_name(tool) {
            declared.insert(tool.trim().to_string());
        }
    }

    declared.into_iter().collect()
}

fn extract_tool_name_from_decision_event(v: &Value) -> Result<String> {
    for (pointer, label) in [
        (Some("/data/tool"), "data.tool"),
        (Some("/data/tool_name"), "data.tool_name"),
        (None, "tool"),
        (None, "tool_name"),
    ] {
        let value = match pointer {
            Some(path) => v.pointer(path),
            None => v.get(label),
        };
        if let Some(s) = value.and_then(Value::as_str) {
            let tool = s.trim();
            if !tool.is_empty() {
                return Ok(tool.to_string());
            }
            anyhow::bail!("field '{label}' must be non-empty string");
        }
    }

    anyhow::bail!("missing required field: 'data.tool', 'data.tool_name', 'tool', or 'tool_name'")
}

fn extract_tool_classes_from_decision_event(v: &Value) -> BTreeSet<String> {
    match v.pointer("/data/tool_classes") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => BTreeSet::new(),
    }
}

pub(super) async fn normalize_decision_jsonl_to_coverage_jsonl(
    input: &Path,
    output: &Path,
) -> Result<()> {
    let content = tokio::fs::read_to_string(input)
        .await
        .with_context(|| format!("failed to read decision log {}", input.display()))?;

    let mut lines = Vec::new();
    for (lineno, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid json at line {}", lineno + 1))?;
        let tool = extract_tool_name_from_decision_event(&value)
            .with_context(|| format!("measurement error at line {}", lineno + 1))?;
        let tool_classes = extract_tool_classes_from_decision_event(&value)
            .into_iter()
            .collect::<Vec<_>>();

        lines.push(serde_json::to_string(&json!({
            "tool": tool,
            "tool_classes": tool_classes,
        }))?);
    }

    let mut normalized = lines.join("\n");
    if !normalized.is_empty() {
        normalized.push('\n');
    }

    tokio::fs::write(output, normalized)
        .await
        .with_context(|| {
            format!(
                "failed to write normalized coverage input {}",
                output.display()
            )
        })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::commands::coverage;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/ci/fixtures/coverage")
            .join(name)
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_extracts_nested_tool_and_classes() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("normalized.jsonl");

        normalize_decision_jsonl_to_coverage_jsonl(
            &fixture_path("decision_event_basic.jsonl"),
            &out,
        )
        .await
        .expect("normalization should succeed");

        let content = std::fs::read_to_string(&out).unwrap();
        let line = content.lines().next().unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["tool"], "assay_policy_decide");
        assert_eq!(value["tool_classes"], json!(["sink:network"]));
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_accepts_data_tool_name_fallback() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("decision.jsonl");
        let out = dir.path().join("normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"data":{"tool_name":"web_search_alt"}}
"#,
        )
        .unwrap();

        normalize_decision_jsonl_to_coverage_jsonl(&input, &out)
            .await
            .expect("data.tool_name fallback should succeed");

        let content = std::fs::read_to_string(&out).unwrap();
        let line = content.lines().next().unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["tool"], "web_search_alt");
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_accepts_top_level_tool_name_fallback() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("decision.jsonl");
        let out = dir.path().join("normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"tool_name":"web_search"}
"#,
        )
        .unwrap();

        normalize_decision_jsonl_to_coverage_jsonl(&input, &out)
            .await
            .expect("top-level tool_name fallback should succeed");

        let content = std::fs::read_to_string(&out).unwrap();
        let line = content.lines().next().unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["tool"], "web_search");
        assert_eq!(value["tool_classes"], json!([]));
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_rejects_missing_tool_fields() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("decision.jsonl");
        let out = dir.path().join("normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"decision":"deny"}
"#,
        )
        .unwrap();

        let err = normalize_decision_jsonl_to_coverage_jsonl(&input, &out)
            .await
            .expect_err("missing tool fields must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("missing required field"));
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_and_report_writer_emit_v1_report() {
        let dir = tempdir().unwrap();
        let normalized = dir.path().join("normalized.jsonl");
        let coverage_out = dir.path().join("coverage.json");

        normalize_decision_jsonl_to_coverage_jsonl(
            &fixture_path("decision_event_basic.jsonl"),
            &normalized,
        )
        .await
        .expect("normalization should succeed");

        let exit = coverage::write_generated_coverage_report(
            &normalized,
            &coverage_out,
            &["assay_policy_decide".to_string()],
            "decision_jsonl",
        )
        .await
        .expect("coverage generation should complete");
        assert_eq!(exit, crate::exit_codes::EXIT_SUCCESS);

        let report: Value =
            serde_json::from_str(&std::fs::read_to_string(&coverage_out).unwrap()).unwrap();
        assert_eq!(report["schema_version"], "coverage_report_v1");
        assert_eq!(report["run"]["source"], "decision_jsonl");
        assert_eq!(
            report["tools"]["tools_seen"],
            json!(["assay_policy_decide"])
        );
        assert_eq!(
            report["taxonomy"]["tool_classes_seen"],
            json!(["sink:network"])
        );
    }

    #[test]
    fn mcp_wrap_coverage_collect_declared_tools_ignores_wildcards() {
        let mut policy = McpPolicy::default();
        policy.tools.allow = Some(vec!["assay_policy_decide".into(), "*".into()]);
        policy.tools.deny = Some(vec!["assay_check_args".into(), "exec*".into()]);
        policy.schemas.insert("$defs".into(), json!({}));
        policy
            .schemas
            .insert("assay_check_sequence".into(), json!({"type": "object"}));

        let declared = collect_declared_tools(&policy);
        assert_eq!(
            declared,
            vec![
                "assay_check_args".to_string(),
                "assay_check_sequence".to_string(),
                "assay_policy_decide".to_string(),
            ]
        );
    }
}
