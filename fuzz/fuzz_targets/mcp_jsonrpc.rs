//! Fuzz the stdio JSON-RPC line handler through the same function the server
//! loop calls (`assay_mcp_server::server::handle_line`), not a copy.
//!
//! The oracle below pins the wire contract from
//! `crates/assay-mcp-server/tests/outer_fallback_contract.rs` for arbitrary
//! input lines: silence exactly where the contract demands it, id echo on
//! every response, and a bounded response size. A panic anywhere in the
//! handler fails the target before any assertion runs.

#![no_main]

use assay_mcp_server::server::{
    handle_line, LineOutcome, MAX_RESPONSE_OVERHEAD_BYTES,
};
use libfuzzer_sys::fuzz_target;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde_json::Value;
use std::collections::HashSet;
use std::fmt;

/// Small limit so the oversize branch is reachable with small inputs, exactly
/// like `ASSAY_MCP_MAX_BYTES=100` in `resource_limits.rs`. The handler takes
/// the limit as a parameter, so this exercises the same code path.
const MAX_MSG_BYTES: usize = 4_096;

/// Duplicate detection over the top-level members of one JSON object line.
///
/// Implemented as a serde visitor rather than a string scan so member-name
/// decoding (escapes, `\uXXXX`, surrogate pairs) is serde's own: a scan that
/// compared raw slices would disagree with the typed parser about
/// `{"id":1,"id":2}` spelled with escapes, and the oracle would assert on the
/// wrong tree. Nested members are skipped; only the top level decides.
struct HasDuplicate(bool);

struct HasDuplicateVisitor;

impl<'de> Visitor<'de> for HasDuplicateVisitor {
    type Value = HasDuplicate;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<HasDuplicate, A::Error> {
        let mut seen = HashSet::new();
        let mut duplicate = false;
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key) {
                duplicate = true;
            }
            let _: IgnoredAny = map.next_value()?;
        }
        Ok(HasDuplicate(duplicate))
    }
}

impl<'de> serde::Deserialize<'de> for HasDuplicate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_map(HasDuplicateVisitor)
    }
}

/// True when `line` is a JSON object with a repeated top-level member name.
/// Only called when `line` already parses as a `Value` object, so a `false`
/// here means "parsed object, no duplicates" rather than "unparsable".
fn has_duplicate_top_level_member(line: &str) -> bool {
    serde_json::from_str::<HasDuplicate>(line)
        .map(|found| found.0)
        .unwrap_or(false)
}

/// The id the contract says an output must carry: the request's own id.
/// `None` here means "no output is allowed", never "null is allowed".
fn expected_id(line: &str, value: &Value) -> Option<Value> {
    if line.len() > MAX_MSG_BYTES {
        // Oversize refusal carries id null, but that is a `Respond`, not silence.
        return None;
    }
    let object = value.as_object()?;
    if has_duplicate_top_level_member(line) {
        return None;
    }
    let id = object.get("id")?;
    if object.get("method").and_then(Value::as_str) == Some("notifications/initialized") {
        // The one request-shaped line that stays silent, pre-existing.
        return None;
    }
    Some(id.clone())
}

fn assert_response_shape(text: &str, expected: &Value, line: &str) {
    let response: Value =
        serde_json::from_str(text).expect("handler response must be one JSON value");
    let object = response
        .as_object()
        .expect("handler response must be an object");
    assert_eq!(
        object.get("jsonrpc"),
        Some(&Value::String("2.0".to_string())),
        "line: {line}"
    );
    let id = object.get("id").expect("response must carry id");
    assert_eq!(id, expected, "response id must echo the request id; line: {line}");
    // Exactly one of result/error: the response struct skips whichever is absent.
    assert!(
        object.get("result").is_some() ^ object.get("error").is_some(),
        "exactly one of result/error; line: {line}"
    );
    assert!(
        text.len() <= line.len() + MAX_RESPONSE_OVERHEAD_BYTES,
        "response ({} bytes) exceeds input ({} bytes) + overhead; line: {line}",
        text.len(),
        line.len()
    );
}

fuzz_target!(|data: &[u8]| {
    // Non-UTF-8 stdin never reaches the handler: `BufRead::lines` rejects it
    // first. Only UTF-8 inputs exercise this path.
    let line = match core::str::from_utf8(data) {
        Ok(line) => line,
        Err(_) => return,
    };

    let first = handle_line(line, MAX_MSG_BYTES, "fuzz");
    // Determinism: same input, same outcome. Compare twice, since the plan
    // carries owned values the response path serializes.
    let second = handle_line(line, MAX_MSG_BYTES, "fuzz");
    match (&first, &second) {
        (LineOutcome::Silent, LineOutcome::Silent) => {}
        (LineOutcome::Respond(a), LineOutcome::Respond(b)) => assert_eq!(a, b),
        (LineOutcome::ExecuteTool(a), LineOutcome::ExecuteTool(b)) => {
            assert_eq!(a.response_id, b.response_id);
            assert_eq!(a.name, b.name);
            assert_eq!(a.arguments, b.arguments);
            assert_eq!(a.params, b.params);
        }
        _ => panic!("line handler is not deterministic for {line:?}"),
    }

    let value: Option<Value> = serde_json::from_str(line).ok();
    // Oversize lines always answer with id null, before any parsing.
    if line.len() > MAX_MSG_BYTES {
        match &first {
            LineOutcome::Respond(text) => {
                assert_response_shape(text, &Value::Null, line);
            }
            _ => panic!("oversize line must be refused, not silent: {line:?}"),
        }
        return;
    }

    match &first {
        LineOutcome::Silent => {
            // Silence is only allowed for a reason the contract names.
            let justified = match &value {
                // Not a JSON value, or not an object: the typed parse fails too.
                None | Some(Value::Null) | Some(Value::Bool(_)) | Some(Value::Number(_))
                | Some(Value::String(_)) | Some(Value::Array(_)) => true,
                Some(Value::Object(object)) => {
                    has_duplicate_top_level_member(line)
                        || !object.contains_key("id")
                        || object.get("method").and_then(Value::as_str)
                            == Some("notifications/initialized")
                        // Structurally not a request (missing or mistyped
                        // `jsonrpc`/`method`): the typed parse rejects it.
                        || object.get("method").and_then(Value::as_str).is_none()
                        || object.get("jsonrpc").and_then(Value::as_str).is_none()
                }
            };
            assert!(justified, "handler swallowed a respondable line: {line:?}");
        }
        LineOutcome::Respond(text) => {
            let expected = expected_id(line, value.as_ref().expect("responded line parses"))
                .expect("responded line must have a single id");
            assert_response_shape(text, &expected, line);
        }
        LineOutcome::ExecuteTool(plan) => {
            let expected = expected_id(line, value.as_ref().expect("planned line parses"))
                .expect("planned line must have a single id");
            assert_eq!(
                plan.response_id,
                Some(expected),
                "planned tools/call must echo the request id; line: {line:?}"
            );
        }
    }
});
