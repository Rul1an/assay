//! Unique-member JSON ingress for live MCP proxy lines.
//!
//! `serde_json::Value` collapses duplicate object members last-value-wins, so a
//! later parse of the same bytes can disagree with the tree a policy decision
//! already used. This boundary refuses duplicates in one pass and applies the
//! transcript line-byte ceiling before building a tree.

use super::era::UniqueValue;
use super::ingest::McpTranscriptLimits;
use serde_json::Value;

/// Why a client line cannot be authorized.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum UniqueJsonError {
    DuplicateMember,
    Malformed,
    LineTooLong {
        limit: u64,
    },
    /// Valid JSON that is not one object on a single transport line.
    InvalidFrame,
    InvalidRequestIdKnown(Value),
}

impl std::fmt::Display for UniqueJsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateMember => f.write_str("JSON object contains a duplicate member"),
            Self::Malformed => f.write_str("malformed JSON"),
            Self::InvalidFrame | Self::InvalidRequestIdKnown(_) => f.write_str("invalid MCP frame"),
            Self::LineTooLong { limit } => {
                write!(f, "JSON line exceeded byte limit of {limit}")
            }
        }
    }
}

impl std::error::Error for UniqueJsonError {}

/// Transport ceilings exclude the optional CRLF terminator.
#[derive(Clone, Copy)]
pub enum FrameDirection {
    Client,
    Server,
}

impl FrameDirection {
    pub fn content_limit(self) -> u64 {
        let limits = McpTranscriptLimits::default();
        match self {
            Self::Client => limits.max_line_bytes,
            Self::Server => limits.max_source_bytes,
        }
    }

    /// Stop the transport when a line exceeds the buffer budget; its remainder
    /// cannot be interpreted as the beginning of a new frame.
    pub fn read_line(
        self,
        reader: &mut impl std::io::BufRead,
        line: &mut String,
    ) -> std::io::Result<usize> {
        use std::io::{BufRead, Read};
        let n = reader.take(self.content_limit() + 3).read_line(line)?;
        self.check_wire_bytes(n)?;
        Ok(n)
    }

    pub fn check_wire_bytes(self, n: usize) -> std::io::Result<()> {
        if n as u64 > self.content_limit() + 2 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "MCP transport frame exceeds byte budget",
            ));
        }
        Ok(())
    }
}

/// Bound the correlation history for sessions whose authorization uses live tool lists.
#[derive(Default)]
pub struct SessionRequestIds(std::collections::HashSet<[u8; 32]>);

impl SessionRequestIds {
    pub fn admit(&mut self, frame: &Value) -> bool {
        use sha2::{Digest, Sha256};
        if frame.get("method").is_none() {
            return true;
        }
        let Some(id) = frame.get("id") else {
            return true;
        };
        let safe_integer = id.as_i64().is_some_and(|n| n.unsigned_abs() < 1 << 53);
        if !id.is_string() && !safe_integer {
            return false;
        }
        self.0.len() < McpTranscriptLimits::default().max_events
            && self
                .0
                .insert(Sha256::digest(id.to_string().as_bytes()).into())
    }
}

/// Parse one client line, refusing duplicate members and oversized lines.
///
/// The line-byte ceiling is [`McpTranscriptLimits::max_line_bytes`] so live
/// proxy ingest and transcript ingest share one number.
pub fn parse_unique_json(line: &str) -> Result<Value, UniqueJsonError> {
    parse_unique_json_with_limit(line, McpTranscriptLimits::default().max_line_bytes)
}

fn parse_unique_json_with_limit(line: &str, limit: u64) -> Result<Value, UniqueJsonError> {
    if (line.len() as u64) > limit {
        return Err(UniqueJsonError::LineTooLong { limit });
    }
    match serde_json::from_str::<UniqueValue>(line) {
        Ok(UniqueValue(value)) => {
            assay_common::unique_value::reject_raw_value_marker(&value)
                .map_err(|_| UniqueJsonError::InvalidFrame)?;
            Ok(value)
        }
        Err(error) => {
            if error.to_string().contains("duplicate member") {
                Err(UniqueJsonError::DuplicateMember)
            } else {
                Err(UniqueJsonError::Malformed)
            }
        }
    }
}

/// Admit one live client frame, preserving the general parser for non-wire users.
/// Arrays inside an object remain valid; root batches and scalars cannot be
/// mistaken for responses and forwarded without request authorization.
pub fn parse_client_frame(line: &str) -> Result<Value, UniqueJsonError> {
    parse_wire_frame(line, FrameDirection::Client.content_limit())
}

/// Read a server frame with room for list pages larger than individual requests.
pub fn parse_server_frame(line: &str) -> Result<Value, UniqueJsonError> {
    parse_wire_frame(line, FrameDirection::Server.content_limit())
}

fn parse_wire_frame(line: &str, limit: u64) -> Result<Value, UniqueJsonError> {
    let content = line
        .strip_suffix("\r\n")
        .or_else(|| line.strip_suffix('\n'))
        .unwrap_or(line);
    if content.len() as u64 > limit {
        return Err(UniqueJsonError::LineTooLong { limit });
    }
    if content.contains(['\r', '\n']) {
        return Err(UniqueJsonError::InvalidFrame);
    }
    let mut value = parse_unique_json_with_limit(content, limit)?;
    if value.get("id").is_some_and(Value::is_object) {
        // The unique-member visitor sees arbitrary-precision number tokens as
        // maps. Recover only an actual JSON number from the original ID span.
        #[derive(serde::Deserialize)]
        struct RawId<'a> {
            #[serde(borrow)]
            id: &'a serde_json::value::RawValue,
        }
        let raw = serde_json::from_str::<RawId<'_>>(content)
            .map_err(|_| UniqueJsonError::InvalidFrame)?
            .id
            .get();
        let number: serde_json::Number = raw.parse().map_err(|_| UniqueJsonError::InvalidFrame)?;
        value["id"] = Value::Number(number);
    }
    let fields = value.as_object().ok_or(UniqueJsonError::InvalidFrame)?;
    if !admits_names(
        &value,
        &["jsonrpc", "id", "method", "params", "result", "error"],
    ) {
        return Err(UniqueJsonError::InvalidFrame);
    }
    let id = fields.get("id");
    if id.is_some_and(|v| !(v.is_null() || v.is_string() || v.is_number())) {
        return Err(UniqueJsonError::InvalidFrame);
    }
    let invalid_request = || {
        id.filter(|v| !v.is_null()).cloned().map_or(
            UniqueJsonError::InvalidFrame,
            UniqueJsonError::InvalidRequestIdKnown,
        )
    };
    if fields.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        if fields.get("method").is_some_and(Value::is_string)
            && !fields.contains_key("result")
            && !fields.contains_key("error")
        {
            return Err(invalid_request());
        }
        return Err(UniqueJsonError::InvalidFrame);
    }
    if let Some(method) = fields.get("method") {
        let method = method
            .as_str()
            .filter(|m| !m.contains(char::is_control))
            .ok_or(UniqueJsonError::InvalidFrame)?;
        if fields.contains_key("result") || fields.contains_key("error") {
            return Err(UniqueJsonError::InvalidFrame);
        }
        if method == "notifications/cancelled" && id.is_some() {
            return Err(invalid_request());
        }
        if matches!(method, "tools/list" | "notifications/cancelled") {
            let reserved: &[&str] = if method == "tools/list" {
                &["cursor", "_meta"]
            } else {
                &["requestId", "reason", "_meta"]
            };
            if value.get("params").is_some_and(|p| {
                !p.is_object()
                    || !admits_names(p, reserved)
                    || (method == "notifications/cancelled"
                        && p.get("reason").is_some_and(|v| !v.is_string()))
            }) {
                return Err(invalid_request());
            }
        }
        if method == "tools/call" {
            if let Some(params) = value["params"].as_object() {
                if !call_arguments_admitted(&value["params"]["arguments"])
                    || !admits_names(
                        &value["params"],
                        &[
                            "name",
                            "arguments",
                            "_meta",
                            "task",
                            "requestState",
                            "inputResponses",
                        ],
                    )
                    || params.get("requestState").is_some_and(|v| !v.is_string())
                    || params.get("inputResponses").is_some_and(|v| !v.is_object())
                    || params
                        .get("name")
                        .and_then(Value::as_str)
                        .is_some_and(|name| name.contains(char::is_control))
                {
                    return Err(invalid_request());
                }
            }
        }
    } else if fields.contains_key("params")
        || fields.contains_key("result") == fields.contains_key("error")
    {
        return Err(UniqueJsonError::InvalidFrame);
    }
    Ok(value)
}

/// Canonical comparison for decision-bearing member names in the live profile.
/// Field names in these objects must be ASCII. An opaque nested map keeps its
/// application-defined keys.
pub(crate) fn wire_spelling(name: &str) -> String {
    name.bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|b| char::from(b.to_ascii_lowercase()))
        .collect()
}

fn plain_call_values(v: &Value) -> bool {
    match v {
        Value::Object(map) => map
            .iter()
            .all(|(name, child)| !name.contains('\0') && plain_call_values(child)),
        Value::Array(items) => items.iter().all(plain_call_values),
        Value::String(text) => !text.contains('\0'),
        _ => true,
    }
}

fn admits_names(v: &Value, reserved: &[&str]) -> bool {
    v.as_object().is_none_or(|map| {
        map.keys().all(|name| {
            if !name.is_ascii() {
                return false;
            }
            let folded = wire_spelling(name);
            reserved.iter().all(|key| {
                name == key
                    || !key
                        .bytes()
                        .filter(u8::is_ascii_alphanumeric)
                        .map(|b| b.to_ascii_lowercase())
                        .eq(folded.bytes())
            })
        })
    })
}

fn call_arguments_admitted(args: &Value) -> bool {
    plain_call_values(args)
        && admits_names(
            args,
            &[
                "_meta",
                "body",
                "headers",
                "path",
                "query",
                "resource",
                "read_only",
                "owner",
                "repo",
                "title",
                "key_title",
                "channel_id",
                "user_id",
                "user",
                "workspace_id",
                "workspace",
                "org",
                "principal",
                "role",
            ],
        )
        && admits_names(
            &args["_meta"],
            &["approval", "delegation", "resource", "tool_call_id"],
        )
        && admits_names(
            &args["_meta"]["delegation"],
            &["delegated_from", "delegation_depth"],
        )
        && admits_names(
            &args["_meta"]["approval"],
            &[
                "approval_id",
                "approver",
                "issued_at",
                "expires_at",
                "scope",
                "bound_tool",
                "bound_resource",
            ],
        )
}

fn live_object_schema(v: &Value) -> bool {
    v.get("type").and_then(Value::as_str) == Some("object")
        && v.get("properties").is_none_or(|p| {
            p.as_object()
                .is_some_and(|o| o.values().all(Value::is_object))
        })
        && v.get("required")
            .is_none_or(|r| r.as_array().is_some_and(|a| a.iter().all(Value::is_string)))
}

/// Whether a mismatched reply makes a list observation ambiguous. A numeric
/// spelling may be treated as the same ID by clients even across JSON types.
/// This check only invalidates a list; accepting a page requires an exact ID.
pub fn ambiguous_list_reply(frame: &Value, pending_id: Option<&Value>) -> bool {
    fn numeric_id(id: &Value) -> Option<f64> {
        match id {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.trim().parse().ok(),
            _ => None,
        }
    }
    frame.pointer("/result/tools").is_some()
        || pending_id
            .and_then(numeric_id)
            .is_some_and(|number| frame.get("id").and_then(numeric_id) == Some(number))
}

/// Bound total retained input across pages as well as each individual frame.
#[derive(Default, Debug)]
pub struct ListByteBudget(u64);

impl ListByteBudget {
    pub fn admit_page(&mut self, bytes: usize) -> bool {
        self.0 = self.0.saturating_add(bytes as u64);
        self.0 <= FrameDirection::Server.content_limit()
    }
}

/// Validate the list result and every tool before either live observer uses a page.
/// Only an absent cursor ends pagination. An empty string is a continuation, and null is invalid.
pub fn live_list_page(frame: &Value) -> Option<(&[Value], Option<&str>)> {
    // Opaque response extensions may be relayed, but some protocol clients
    // discard such envelopes. Only the exact result envelope can renew a pin.
    if !frame
        .as_object()?
        .keys()
        .all(|k| matches!(k.as_str(), "jsonrpc" | "id" | "result"))
    {
        return None;
    }
    let result = frame.get("result")?.as_object()?;
    if frame.get("error").is_some()
        || !result
            .keys()
            .all(|k| matches!(k.as_str(), "tools" | "nextCursor" | "_meta"))
    {
        return None;
    }
    let tools = result.get("tools")?.as_array()?;
    if !tools.iter().all(is_live_tool_definition)
        || result.get("_meta").is_some_and(|v| {
            !v.as_object().is_some_and(|meta| {
                meta.get("progressToken").is_none_or(|t| {
                    t.is_string() || t.as_i64().is_some_and(|n| n.unsigned_abs() < 1 << 53)
                }) && meta
                    .get("io.modelcontextprotocol/related-task")
                    .is_none_or(|t| t.get("taskId").is_some_and(Value::is_string))
            })
        })
    {
        return None;
    }
    let cursor = match result.get("nextCursor") {
        None => None,
        Some(v) => Some(v.as_str()?),
    };
    Some((tools, cursor))
}

/// Fields admitted for live tool identity. Application-defined schema contents
/// are opaque to this structural check.
pub fn is_live_tool_definition(tool: &Value) -> bool {
    let Some(obj) = tool.as_object() else {
        return false;
    };
    obj.keys().all(|k| {
        matches!(
            k.as_str(),
            "name"
                | "title"
                | "description"
                | "inputSchema"
                | "outputSchema"
                | "annotations"
                | "execution"
                | "icons"
                | "_meta"
        ) || k.starts_with("x-")
    }) && obj
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.trim().is_empty() && !name.contains(char::is_control))
        && obj.get("description").is_none_or(Value::is_string)
        && obj.get("title").is_none_or(Value::is_string)
        && obj.get("inputSchema").is_some_and(live_object_schema)
        && obj.get("outputSchema").is_none_or(live_object_schema)
        && obj.get("annotations").is_none_or(|v| {
            v.as_object().is_some_and(|a| {
                a.iter().all(|(key, v)| match key.as_str() {
                    "title" => v.is_string(),
                    "readOnlyHint" | "destructiveHint" | "idempotentHint" | "openWorldHint" => {
                        v.is_boolean()
                    }
                    _ => false,
                })
            })
        })
        && obj.get("_meta").is_none_or(Value::is_object)
        && obj.get("execution").is_none_or(|v| {
            v.as_object().is_some_and(|o| {
                o.iter().all(|(k, v)| {
                    k == "taskSupport"
                        && matches!(v.as_str(), Some("required" | "optional" | "forbidden"))
                })
            })
        })
        && obj.get("icons").is_none_or(|v| {
            v.as_array().is_some_and(|icons| {
                icons.iter().all(|icon| {
                    icon.get("src").is_some_and(Value::is_string)
                        && icon.get("mimeType").is_none_or(Value::is_string)
                        && icon
                            .get("theme")
                            .is_none_or(|v| matches!(v.as_str(), Some("light" | "dark")))
                        && icon.get("sizes").is_none_or(|v| {
                            v.as_array().is_some_and(|a| a.iter().all(Value::is_string))
                        })
                })
            })
        })
}

/// Whether an unparsable line still looks like a JSON-RPC method frame.
///
/// Matches the wrap path's existing method-bearing check so only that class of
/// opaque line changes from "warn and forward" to refuse.
pub fn is_method_bearing_frame(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('{')
        && (trimmed.contains("\"method\"")
            || trimmed.contains("\"params\"")
            || trimmed.contains("\"tool\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn protocol_fields_use_exact_names_and_a_single_line_terminator() {
        assert!(parse_client_frame(r#"{"jsonrpc":"2.0","Method":"ping"}"#).is_err());
        assert!(parse_client_frame("{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\r\n\r\n").is_err());
        assert!(parse_client_frame("{\"jsonrpc\":\"2.0\",\"method\":\"ping\"}\r\n").is_ok());
    }

    #[test]
    fn application_member_names_and_unidentified_errors_are_preserved() {
        let response = r#"{"jsonrpc":"2.0","id":1,"result":{"x":1,"X":2}}"#;
        assert_eq!(
            parse_server_frame(response).unwrap()["result"],
            json!({"x":1,"X":2})
        );
        assert!(
            parse_client_frame(r#"{"jsonrpc":"2.0","error":{"code":1,"message":"example"}}"#)
                .is_ok()
        );
    }

    #[test]
    fn free_form_maps_in_calls_preserve_case_and_metadata_names_are_exact() {
        let call = |args| {
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"example","arguments":args}}).to_string()
        };
        assert!(parse_client_frame(&call(json!({"env":{"PATH":"a","path":"b"}}))).is_ok());
        assert!(parse_client_frame(&call(
            json!({"_meta":{"delegation":{"Delegated_From":"example"}}})
        ))
        .is_err());
        assert!(parse_client_frame(&call(json!({"_meta":{"Tool_Call_Id":"example"}}))).is_err());
    }

    #[test]
    fn live_tool_identity_requires_canonical_input_schema() {
        assert!(is_live_tool_definition(
            &json!({"name":"example","inputSchema":{"type":"object"}})
        ));
        assert!(!is_live_tool_definition(
            &json!({"name":"example","input_schema":{"type":"object"}})
        ));
    }

    #[test]
    fn live_session_ids_are_unique_per_requestor() {
        let mut ids = SessionRequestIds::default();
        assert!(ids.admit(&json!({"method":"ping","id":1})));
        assert!(ids.admit(&json!({"result":{},"id":1})));
        assert!(ids.admit(&json!({"method":"ping","id":"1"})));
        assert!(!ids.admit(&json!({"method":"tools/list","id":1})));
    }

    #[test]
    fn modern_call_parameters_and_extensions_are_preserved() {
        let call = json!({"jsonrpc":"2.0","method":"tools/call","id":"echo","params":{
            "name":"echo","arguments":{},"requestState":"resume","inputResponses":{},
            "vendor/example":{"language":"nl"}}});
        assert_eq!(parse_client_frame(&call.to_string()).unwrap(), call);
    }

    #[test]
    fn ordinary_numeric_tokens_and_typed_invalid_request_ids_round_trip() {
        let line = r#"{"jsonrpc":"2.0","id":18446744073709551617,"method":"ping"}"#;
        assert!(parse_client_frame(line).unwrap()["id"].is_number());
        assert_eq!(
            parse_client_frame(r#"{"method":"ping","id":"client-example"}"#),
            Err(UniqueJsonError::InvalidRequestIdKnown(json!(
                "client-example"
            )))
        );
    }

    #[test]
    fn malformed_request_params_keep_an_unambiguous_request_id() {
        for (method, params) in [
            ("tools/call", json!({"name":"echo","requestState":7})),
            ("notifications/cancelled", json!({"requestId":1})),
        ] {
            let frame = json!({"jsonrpc":"2.0","method":method,"id":2,"params":params});
            assert_eq!(
                parse_client_frame(&frame.to_string()),
                Err(UniqueJsonError::InvalidRequestIdKnown(json!(2)))
            );
        }
    }

    #[test]
    fn cancellation_accepts_an_optional_string_reason() {
        let frame = json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":12,"reason":"client finished"}});
        assert_eq!(parse_client_frame(&frame.to_string()).unwrap(), frame);
    }

    #[test]
    fn list_page_preserves_even_an_empty_opaque_continuation() {
        let page = json!({"jsonrpc":"2.0","id":1,"result":{"tools":[],"nextCursor":""}});
        assert_eq!(live_list_page(&page).unwrap().1, Some(""));
    }

    #[test]
    fn opaque_response_extensions_are_relayable_without_renewing_a_manifest() {
        let line = r#"{"jsonrpc":"2.0","id":12,"result":{"tools":[]},"opaque":true}"#;
        let value = parse_client_frame(line).expect("ordinary response extensions are relayable");
        assert!(live_list_page(&value).is_none());
    }

    #[test]
    fn unique_members_parse() {
        let v = parse_unique_json(r#"{"name":"echo","arguments":{"x":1}}"#).unwrap();
        assert_eq!(v["name"], "echo");
        assert_eq!(v["arguments"]["x"], 1);
    }

    #[test]
    fn duplicate_members_are_refused() {
        let err = parse_unique_json(r#"{"name":"alpha","name":"bravo"}"#).unwrap_err();
        assert_eq!(err, UniqueJsonError::DuplicateMember);
        let rendered = format!("{err}");
        assert!(!rendered.contains("alpha") && !rendered.contains("bravo"));
    }

    #[test]
    fn malformed_is_refused() {
        let err = parse_unique_json(r#"{"name":"echo""#).unwrap_err();
        assert_eq!(err, UniqueJsonError::Malformed);
    }

    #[test]
    fn oversized_line_is_refused_before_a_second_parse() {
        let limit = McpTranscriptLimits::default().max_line_bytes;
        let line = "x".repeat((limit as usize) + 1);
        assert_eq!(
            parse_unique_json(&line),
            Err(UniqueJsonError::LineTooLong { limit })
        );
    }

    #[test]
    fn method_bearing_detects_the_wrap_heuristic() {
        assert!(is_method_bearing_frame(
            r#"{"jsonrpc":"2.0","method":"tools/call""#
        ));
        assert!(!is_method_bearing_frame("not-json"));
        assert_eq!(json!({"name": "echo"})["name"], "echo");
    }
}
