use assay_core::model::{LlmResponse, ToolCallRecord};

fn parse_best_effort_entry(v: &serde_json::Value, idx: usize) -> Option<ToolCallRecord> {
    if let Ok(call) = serde_json::from_value::<ToolCallRecord>(v.clone()) {
        return Some(call);
    }
    let obj = v.as_object()?;
    let tool_name = obj
        .get("tool_name")
        .or(obj.get("tool"))
        .and_then(|x| x.as_str())
        .map(ToString::to_string)?;

    let args = obj
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let id = obj
        .get("id")
        .and_then(|x| x.as_str())
        .map(ToString::to_string)
        .unwrap_or_else(|| format!("legacy-{}", idx));
    let index = obj
        .get("index")
        .and_then(|x| x.as_u64())
        .map(|x| x as usize)
        .unwrap_or(idx);
    let ts_ms = obj
        .get("ts_ms")
        .or(obj.get("timestamp"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    let result = obj.get("result").cloned();
    let error = obj.get("error").cloned();

    Some(ToolCallRecord {
        id,
        tool_name,
        args,
        result,
        error,
        index,
        ts_ms,
    })
}

/// Canonical-only extraction: deserialize exact ToolCallRecord list or return empty.
pub(crate) fn extract_tool_calls_canonical_or_empty(resp: &LlmResponse) -> Vec<ToolCallRecord> {
    let Some(val) = resp.meta.get("tool_calls") else {
        return Vec::new();
    };
    serde_json::from_value(val.clone()).unwrap_or_default()
}

/// Best-effort extraction: canonical parse first, then lenient legacy entry mapping.
pub(crate) fn extract_tool_calls_best_effort(resp: &LlmResponse) -> Vec<ToolCallRecord> {
    let Some(val) = resp.meta.get("tool_calls") else {
        return Vec::new();
    };
    if let Ok(calls) = serde_json::from_value::<Vec<ToolCallRecord>>(val.clone()) {
        return calls;
    }
    val.as_array()
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(idx, entry)| parse_best_effort_entry(entry, idx))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_or_empty_parses_only_canonical_records() {
        let canonical = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [{
                    "id": "c1",
                    "tool_name": "exec",
                    "args": {"command": "ls"},
                    "result": {"ok": true},
                    "error": null,
                    "index": 0,
                    "ts_ms": 10
                }]
            }),
            ..Default::default()
        };
        let calls = extract_tool_calls_canonical_or_empty(&canonical);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "exec");

        let malformed = LlmResponse {
            meta: serde_json::json!({"tool_calls": {"tool_name": "exec"}}),
            ..Default::default()
        };
        assert!(extract_tool_calls_canonical_or_empty(&malformed).is_empty());
    }

    #[test]
    fn best_effort_accepts_legacy_and_skips_unparsable_entries() {
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [
                    {"tool": "a", "args": {"x": 1}},
                    {"args": {"missing": true}},
                    {"tool_name": "b", "args": ["x"], "error": {"code": "E_FAIL"}}
                ]
            }),
            ..Default::default()
        };

        let calls = extract_tool_calls_best_effort(&resp);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool_name, "a");
        assert_eq!(calls[0].id, "legacy-0");
        assert_eq!(calls[0].index, 0);
        assert_eq!(calls[1].tool_name, "b");
        assert_eq!(calls[1].args, serde_json::json!(["x"]));
        assert_eq!(calls[1].error, Some(serde_json::json!({"code":"E_FAIL"})));
    }

    #[test]
    fn extractors_return_empty_when_tool_calls_missing() {
        let resp = LlmResponse {
            meta: serde_json::json!({}),
            ..Default::default()
        };
        assert!(extract_tool_calls_canonical_or_empty(&resp).is_empty());
        assert!(extract_tool_calls_best_effort(&resp).is_empty());
    }
}
