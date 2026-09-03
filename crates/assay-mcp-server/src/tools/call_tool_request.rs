//! Classify a parsed `tools/call` params envelope before tool execution.
//!
//! Protocol faults stay value-free: fixed messages and complete `data` objects with only
//! a stable `kind`. Hostile tool names and argument bodies never enter the public error.
//!
//! Tool membership is not restated here: [`is_known_tool`] reads the names already
//! advertised by [`super::list_tools`].

use serde_json::{json, Value};

/// JSON-RPC invalid params. Used for both unknown-tool and malformed CallToolRequest.
pub const ERROR_INVALID_PARAMS: i32 = -32602;

const DATA_KIND_MALFORMED_CALL: &str = "malformed_call";
const DATA_KIND_UNKNOWN_TOOL: &str = "unknown_tool";

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
            Self::MalformedCall => json!({ "kind": DATA_KIND_MALFORMED_CALL }),
            Self::UnknownTool => json!({ "kind": DATA_KIND_UNKNOWN_TOOL }),
        }
    }
}

/// Known tool + arguments ready for execution.
#[derive(Debug)]
pub struct CallToolDispatch {
    pub name: String,
    pub arguments: Value,
}

/// True iff `name` is advertised by [`super::list_tools`].
///
/// This is the membership gate for protocol classification. It must not grow a
/// second literal tool-name list beside `list_tools`.
pub fn is_known_tool(name: &str) -> bool {
    super::list_tools()
        .iter()
        .any(|tool| tool.get("name").and_then(Value::as_str) == Some(name))
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
    use std::collections::BTreeSet;

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
            CallToolProtocolFault::UnknownTool.data().get("kind"),
            Some(&json!("malformed_call")),
            "UnknownTool.data must never collapse to malformed_call"
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
        assert_eq!(
            unknown.data(),
            json!({ "kind": "unknown_tool" }),
            "UnknownTool wire data must stay distinct from malformed_call"
        );
    }

    #[test]
    fn classify_accepts_known_tool_with_default_arguments() {
        let dispatch =
            classify_call_tool_params(Some(&json!({ "name": "assay_check_args" }))).unwrap();
        assert_eq!(dispatch.name, "assay_check_args");
        assert_eq!(dispatch.arguments, json!({}));
    }

    #[test]
    fn classifier_membership_tracks_every_advertised_tool() {
        let advertised: BTreeSet<String> = super::super::list_tools()
            .iter()
            .map(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .expect("list_tools entry must have a string name")
                    .to_string()
            })
            .collect();
        assert!(
            !advertised.is_empty(),
            "list_tools must advertise at least one tool"
        );

        for name in &advertised {
            assert!(
                is_known_tool(name),
                "advertised tool {name} missing from classifier membership"
            );
            assert!(
                classify_call_tool_params(Some(&json!({
                    "name": name,
                    "arguments": {}
                })))
                .is_ok(),
                "advertised tool {name} must classify as known; dropping it from \
                 membership (or diverging from list_tools) must fail here"
            );
        }

        // Symmetric drift: accepting an unadvertised name must also fail this test.
        assert!(!advertised.contains("not_an_advertised_tool"));
        assert!(!is_known_tool("not_an_advertised_tool"));
        assert_eq!(
            classify_call_tool_params(Some(&json!({
                "name": "not_an_advertised_tool",
                "arguments": {}
            })))
            .unwrap_err(),
            CallToolProtocolFault::UnknownTool
        );
    }

    #[test]
    fn dropping_one_advertised_name_from_membership_is_detectable() {
        let advertised: Vec<String> = super::super::list_tools()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(Value::as_str).map(str::to_string))
            .collect();
        assert!(advertised.len() >= 2, "need at least two advertised tools");

        // Simulate a divergent membership list that omits the first advertised tool.
        let omitted = &advertised[0];
        let drifted: BTreeSet<&str> = advertised[1..].iter().map(String::as_str).collect();
        assert!(
            !drifted.contains(omitted.as_str()),
            "fixture must omit one advertised name"
        );

        // Production membership must still accept the omitted name; if is_known_tool
        // were rebuilt as `drifted.contains(name)`, this assertion is what fails.
        assert!(
            is_known_tool(omitted),
            "production membership dropped advertised tool {omitted}"
        );
        assert!(
            classify_call_tool_params(Some(&json!({
                "name": omitted,
                "arguments": {}
            })))
            .is_ok(),
            "classifier must keep accepting every list_tools name; omitted={omitted}"
        );
    }
}
