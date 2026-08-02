use assay_core::model::{LlmResponse, ToolCallRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MalformedToolCallEvidence;

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

/// Canonical-only extraction: absence is an empty trace; malformed presence is an error.
pub(crate) fn extract_tool_calls_canonical(
    resp: &LlmResponse,
) -> Result<Vec<ToolCallRecord>, MalformedToolCallEvidence> {
    let Some(val) = resp.meta.get("tool_calls") else {
        return Ok(Vec::new());
    };
    serde_json::from_value(val.clone()).map_err(|_| MalformedToolCallEvidence)
}

/// Best-effort extraction preserves valid legacy entries but rejects malformed presence.
pub(crate) fn extract_tool_calls_best_effort(
    resp: &LlmResponse,
) -> Result<Vec<ToolCallRecord>, MalformedToolCallEvidence> {
    let Some(val) = resp.meta.get("tool_calls") else {
        return Ok(Vec::new());
    };
    if let Ok(calls) = serde_json::from_value::<Vec<ToolCallRecord>>(val.clone()) {
        return Ok(calls);
    }
    val.as_array()
        .ok_or(MalformedToolCallEvidence)?
        .iter()
        .enumerate()
        .map(|(idx, entry)| parse_best_effort_entry(entry, idx).ok_or(MalformedToolCallEvidence))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_extraction_rejects_malformed_presence() {
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
        let calls = extract_tool_calls_canonical(&canonical).unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].tool_name, "exec");

        let malformed = LlmResponse {
            meta: serde_json::json!({"tool_calls": {"tool_name": "exec"}}),
            ..Default::default()
        };
        assert!(extract_tool_calls_canonical(&malformed).is_err());
    }

    #[test]
    fn best_effort_accepts_legacy_entries_and_rejects_malformed_entries() {
        let resp = LlmResponse {
            meta: serde_json::json!({
                "tool_calls": [
                    {"tool": "a", "args": {"x": 1}},
                    {"tool_name": "b", "args": ["x"], "error": {"code": "E_FAIL"}}
                ]
            }),
            ..Default::default()
        };

        let calls = extract_tool_calls_best_effort(&resp).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool_name, "a");
        assert_eq!(calls[0].id, "legacy-0");
        assert_eq!(calls[0].index, 0);
        assert_eq!(calls[1].tool_name, "b");
        assert_eq!(calls[1].args, serde_json::json!(["x"]));
        assert_eq!(calls[1].error, Some(serde_json::json!({"code":"E_FAIL"})));

        let malformed = LlmResponse {
            meta: serde_json::json!({"tool_calls": [{"args": {"missing": true}}]}),
            ..Default::default()
        };
        assert!(extract_tool_calls_best_effort(&malformed).is_err());
    }

    #[test]
    fn extractors_return_empty_when_tool_calls_missing() {
        let resp = LlmResponse {
            meta: serde_json::json!({}),
            ..Default::default()
        };
        assert!(extract_tool_calls_canonical(&resp).unwrap().is_empty());
        assert!(extract_tool_calls_best_effort(&resp).unwrap().is_empty());
    }
}
