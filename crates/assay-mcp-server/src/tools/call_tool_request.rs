//! Classify a parsed `tools/call` params envelope before tool execution.
//!
//! Protocol faults stay value-free: fixed messages and complete `data` objects with only
//! a stable `kind`. Hostile tool names and argument bodies never enter the public error.

use serde_json::{json, Value};

/// JSON-RPC invalid params. Used for both unknown-tool and malformed CallToolRequest.
pub const ERROR_INVALID_PARAMS: i32 = -32602;

/// Request-envelope failure for `tools/call` after JSON-RPC parse succeeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallToolProtocolFault {
    /// Params missing, non-object, or fail CallToolRequest shape (name/arguments).
    MalformedCall,
    /// `name` is a string but not a tool this server exposes.
    UnknownTool,
}

impl CallToolProtocolFault {
    pub const fn code(self) -> i32 {
        ERROR_INVALID_PARAMS
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::MalformedCall => "Invalid params",
            Self::UnknownTool => "Unknown tool",
        }
    }

    /// Complete public `error.data` object. Equality-sensitive: no extra fields.
    pub fn data(self) -> Value {
        match self {
            Self::MalformedCall => json!({ "kind": "malformed_call" }),
            Self::UnknownTool => json!({ "kind": "unknown_tool" }),
        }
    }
}

/// Known tool + arguments ready for execution.
#[derive(Debug)]
pub struct CallToolDispatch {
    pub name: String,
    pub arguments: Value,
}

/// True iff `name` is a tool this process will execute.
///
/// Single membership rule for protocol classification and dispatch.
pub fn is_known_tool(name: &str) -> bool {
    matches!(
        name,
        "assay_check_args"
            | "assay_check_sequence"
            | "assay_policy_decide"
            | "assay_check_coverage"
            | "assay_explain_trace"
    ) || {
        #[cfg(feature = "test-outbound")]
        {
            name == "assay_test_outbound"
        }
        #[cfg(not(feature = "test-outbound"))]
        {
            false
        }
    }
}

/// Classify `tools/call` params after the JSON-RPC request object is parsed.
///
/// Does not reflect `name` or argument contents into the fault.
pub fn classify_call_tool_params(
    params: Option<&Value>,
) -> Result<CallToolDispatch, CallToolProtocolFault> {
    let Some(params) = params else {
        return Err(CallToolProtocolFault::MalformedCall);
    };
    let Some(obj) = params.as_object() else {
        return Err(CallToolProtocolFault::MalformedCall);
    };
    let Some(name) = obj.get("name").and_then(Value::as_str) else {
        return Err(CallToolProtocolFault::MalformedCall);
    };
    let arguments = match obj.get("arguments") {
        None => json!({}),
        Some(value) if value.is_object() => value.clone(),
        Some(_) => return Err(CallToolProtocolFault::MalformedCall),
    };
    if !is_known_tool(name) {
        return Err(CallToolProtocolFault::UnknownTool);
    }
    Ok(CallToolDispatch {
        name: name.to_string(),
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_and_unknown_data_are_complete_and_distinct() {
        assert_eq!(
            CallToolProtocolFault::MalformedCall.data(),
            json!({ "kind": "malformed_call" })
        );
        assert_eq!(
            CallToolProtocolFault::UnknownTool.data(),
            json!({ "kind": "unknown_tool" })
        );
        assert_ne!(
            CallToolProtocolFault::MalformedCall.data(),
            CallToolProtocolFault::UnknownTool.data()
        );
    }

    #[test]
    fn classify_rejects_envelope_shapes_without_reflection() {
        let hostile = format!("HEAD{}TAIL", "x".repeat(100));
        assert_eq!(
            classify_call_tool_params(None).unwrap_err(),
            CallToolProtocolFault::MalformedCall
        );
        assert_eq!(
            classify_call_tool_params(Some(&json!(1))).unwrap_err(),
            CallToolProtocolFault::MalformedCall
        );
        assert_eq!(
            classify_call_tool_params(Some(&json!({ "arguments": {} }))).unwrap_err(),
            CallToolProtocolFault::MalformedCall
        );
        assert_eq!(
            classify_call_tool_params(Some(&json!({ "name": 7, "arguments": {} }))).unwrap_err(),
            CallToolProtocolFault::MalformedCall
        );
        assert_eq!(
            classify_call_tool_params(Some(
                &json!({ "name": "assay_check_args", "arguments": "nope" })
            ))
            .unwrap_err(),
            CallToolProtocolFault::MalformedCall
        );
        let unknown = classify_call_tool_params(Some(&json!({
            "name": hostile,
            "arguments": { "leak": hostile }
        })))
        .unwrap_err();
        assert_eq!(unknown, CallToolProtocolFault::UnknownTool);
        let wire = serde_json::to_string(&unknown.data()).unwrap();
        assert!(!wire.contains("HEAD"));
        assert!(!wire.contains("TAIL"));
        assert_eq!(unknown.message(), "Unknown tool");
    }

    #[test]
    fn classify_accepts_known_tool_with_default_arguments() {
        let dispatch =
            classify_call_tool_params(Some(&json!({ "name": "assay_check_args" }))).unwrap();
        assert_eq!(dispatch.name, "assay_check_args");
        assert_eq!(dispatch.arguments, json!({}));
    }
}
