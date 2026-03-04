use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Result};
use chrono::{SecondsFormat, Utc};
use serde_json::{json, Value};

use crate::cli::args::CoverageArgs;

fn extract_tool_name(v: &Value) -> Result<String, String> {
    if let Some(s) = v.get("tool").and_then(|x| x.as_str()) {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
        return Err("field 'tool' must be non-empty string".to_string());
    }

    if let Some(s) = v.get("tool_name").and_then(|x| x.as_str()) {
        let t = s.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
        return Err("field 'tool_name' must be non-empty string".to_string());
    }

    Err("missing required field: 'tool' or 'tool_name'".to_string())
}

fn extract_tool_classes(v: &Value) -> BTreeSet<String> {
    match v.get("tool_classes") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|x| x.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => BTreeSet::new(),
    }
}

fn build_findings(tools_unknown: &[String], tools_missing_taxonomy: &[String]) -> Vec<Value> {
    let mut findings = Vec::new();

    for tool in tools_unknown {
        findings.push(json!({
            "kind": "unknown_tool",
            "severity": "note",
            "message": format!("tool '{}' was seen but not declared", tool),
            "tool": tool,
            "count": 1
        }));
    }

    for tool in tools_missing_taxonomy {
        findings.push(json!({
            "kind": "missing_taxonomy",
            "severity": "note",
            "message": format!("tool '{}' was seen but has no taxonomy entry", tool),
            "tool": tool,
            "count": 1
        }));
    }

    findings.sort_by(|a, b| {
        let ak = (
            a.get("severity")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            a.get("kind").and_then(Value::as_str).unwrap_or_default(),
            a.get("message").and_then(Value::as_str).unwrap_or_default(),
            a.get("tool").and_then(Value::as_str).unwrap_or_default(),
            a.get("tool_class")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            a.get("count").and_then(Value::as_i64).unwrap_or_default(),
        );
        let bk = (
            b.get("severity")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            b.get("kind").and_then(Value::as_str).unwrap_or_default(),
            b.get("message").and_then(Value::as_str).unwrap_or_default(),
            b.get("tool").and_then(Value::as_str).unwrap_or_default(),
            b.get("tool_class")
                .and_then(Value::as_str)
                .unwrap_or_default(),
            b.get("count").and_then(Value::as_i64).unwrap_or_default(),
        );
        ak.cmp(&bk)
    });

    findings
}

pub async fn build_coverage_report(args: &CoverageArgs) -> Result<Value> {
    let input = args
        .input
        .as_ref()
        .ok_or_else(|| anyhow!("--input is required in coverage generator mode"))?;

    let file_content = tokio::fs::read_to_string(input)
        .await
        .map_err(|e| anyhow!("failed to read input file {}: {e}", input.display()))?;

    let declared_tools: BTreeSet<String> = args
        .declared_tools
        .iter()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    let mut tools_in_order = Vec::new();
    let mut classes_by_tool: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();

    for (lineno, line) in file_content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line)
            .map_err(|e| anyhow!("invalid json at line {}: {}", lineno + 1, e))?;

        let tool = extract_tool_name(&value)
            .map_err(|e| anyhow!("measurement error at line {}: {}", lineno + 1, e))?;
        let classes = extract_tool_classes(&value);

        tools_in_order.push(tool.clone());
        classes_by_tool.entry(tool).or_default().extend(classes);
    }

    let tools_seen: BTreeSet<String> = tools_in_order.iter().cloned().collect();
    let tools_unknown: BTreeSet<String> = tools_seen.difference(&declared_tools).cloned().collect();

    let mut tool_classes_seen = BTreeSet::new();
    let mut tool_classes_missing = BTreeSet::new();
    for tool in &tools_seen {
        let classes = classes_by_tool.get(tool).cloned().unwrap_or_default();
        if classes.is_empty() {
            tool_classes_missing.insert(tool.clone());
        } else {
            tool_classes_seen.extend(classes);
        }
    }

    let mut route_counts: BTreeMap<(String, String), u64> = BTreeMap::new();
    for pair in tools_in_order.windows(2) {
        let key = (pair[0].clone(), pair[1].clone());
        *route_counts.entry(key).or_insert(0) += 1;
    }

    let routes_seen: Vec<Value> = route_counts
        .into_iter()
        .map(|((from, to), count)| {
            json!({
                "from": from,
                "to": to,
                "count": count
            })
        })
        .collect();

    let tools_unknown_vec: Vec<String> = tools_unknown.into_iter().collect();
    let tool_classes_missing_vec: Vec<String> = tool_classes_missing.into_iter().collect();
    let findings = build_findings(&tools_unknown_vec, &tool_classes_missing_vec);

    Ok(json!({
        "schema_version": "coverage_report_v1",
        "report_version": "1",
        "run": {
            "assay_version": env!("CARGO_PKG_VERSION"),
            "generated_at": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            "source": "jsonl"
        },
        "tools": {
            "tools_seen": tools_seen.into_iter().collect::<Vec<_>>(),
            "tools_declared": declared_tools.into_iter().collect::<Vec<_>>(),
            "tools_unknown": tools_unknown_vec
        },
        "taxonomy": {
            "tool_classes_seen": tool_classes_seen.into_iter().collect::<Vec<_>>(),
            "tool_classes_missing": tool_classes_missing_vec
        },
        "routes": {
            "routes_seen": routes_seen
        },
        "findings": findings
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_tool_prefers_tool_over_tool_name() {
        let value = json!({
            "tool": "web_search",
            "tool_name": "web_search_alt"
        });
        let tool = extract_tool_name(&value).expect("tool should parse");
        assert_eq!(tool, "web_search");
    }

    #[test]
    fn extract_tool_name_rejects_missing_fields() {
        let value = json!({
            "decision": "deny"
        });
        let err = extract_tool_name(&value).expect_err("missing tool fields should fail");
        assert!(err.contains("missing required field"));
    }
}
