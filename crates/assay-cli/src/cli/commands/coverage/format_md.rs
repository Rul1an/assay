use serde_json::Value;

fn as_string_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn declared_tools_basis_note(source: &str) -> &'static str {
    match source {
        "decision_jsonl" => {
            "Policy-derived declarations enumerate explicit tool names; wildcard patterns are excluded, so the set is a floor on policy reach."
        }
        "jsonl" => {
            "Caller-supplied declarations are literal values; wildcard-looking values are not expanded or filtered."
        }
        _ => "The declaration basis is unknown; inspect `run.source` before interpreting the set.",
    }
}

pub(crate) fn render_coverage_markdown(
    report: &Value,
    routes_top: usize,
) -> Result<String, String> {
    let schema_version = report
        .get("schema_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing schema_version".to_string())?;
    let report_version = report
        .get("report_version")
        .and_then(Value::as_str)
        .unwrap_or("1");
    let source = report
        .pointer("/run/source")
        .and_then(Value::as_str)
        .unwrap_or("unknown");

    let tools_seen = as_string_array(report.pointer("/tools/tools_seen"));
    let tools_declared = as_string_array(report.pointer("/tools/tools_declared"));
    let tools_unknown = as_string_array(report.pointer("/tools/tools_unknown"));
    let taxonomy_missing = as_string_array(report.pointer("/taxonomy/tool_classes_missing"));

    let mut routes: Vec<(String, String, u64)> = match report.pointer("/routes/routes_seen") {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|item| {
                let from = item.get("from")?.as_str()?.to_string();
                let to = item.get("to")?.as_str()?.to_string();
                let count = item.get("count")?.as_u64()?;
                Some((from, to, count))
            })
            .collect(),
        _ => Vec::new(),
    };

    routes.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut out = String::new();
    out.push_str("# Coverage Report\n\n");
    out.push_str(&format!("- Schema: `{}`\n", schema_version));
    out.push_str(&format!("- Report version: `{}`\n", report_version));
    out.push_str(&format!("- Source: `{}`\n\n", source));

    out.push_str("## Tools\n\n");
    out.push_str("| Metric | Value |\n");
    out.push_str("| --- | ---: |\n");
    out.push_str(&format!("| tools_seen | {} |\n", tools_seen.len()));
    out.push_str(&format!("| tools_declared | {} |\n", tools_declared.len()));
    out.push_str(&format!("| tools_unknown | {} |\n\n", tools_unknown.len()));

    out.push_str("## Unknown Tools\n\n");
    if tools_unknown.is_empty() {
        out.push_str("- _none_\n\n");
    } else {
        for tool in &tools_unknown {
            out.push_str(&format!("- `{}`\n", tool));
        }
        out.push('\n');
    }

    out.push_str("## Taxonomy Missing\n\n");
    if taxonomy_missing.is_empty() {
        out.push_str("- _none_\n\n");
    } else {
        for tool in &taxonomy_missing {
            out.push_str(&format!("- `{}`\n", tool));
        }
        out.push('\n');
    }

    out.push_str("## Top Routes\n\n");
    if routes_top == 0 {
        out.push_str("(routes hidden; --routes-top=0)\n\n");
    } else {
        out.push_str("| From | To | Count |\n");
        out.push_str("| --- | --- | ---: |\n");
        for (from, to, count) in routes.into_iter().take(routes_top) {
            out.push_str(&format!("| `{}` | `{}` | {} |\n", from, to, count));
        }
        out.push('\n');
    }

    out.push_str("## Notes\n\n");
    out.push_str(&format!("- {}\n", declared_tools_basis_note(source)));
    out.push_str("- Routes are adjacent tool-call edges in observed order (v1).\n");
    out.push_str("- This markdown is a presentation of `coverage_report_v1`; enforcement behavior is unchanged.\n");

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn markdown_states_policy_enumeration_floor_for_decision_source() {
        let report = json!({
            "schema_version": "coverage_report_v1",
            "report_version": "1",
            "run": {"source": "decision_jsonl"},
            "tools": {
                "tools_seen": [],
                "tools_declared": [],
                "tools_unknown": []
            },
            "taxonomy": {"tool_classes_missing": []},
            "routes": {"routes_seen": []}
        });

        let markdown = render_coverage_markdown(&report, 0).expect("report should render");
        assert!(markdown.contains(
            "Policy-derived declarations enumerate explicit tool names; wildcard patterns are excluded, so the set is a floor on policy reach."
        ));
    }
}
