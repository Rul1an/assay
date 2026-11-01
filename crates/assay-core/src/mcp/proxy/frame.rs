//! Rewrites what redaction changed in a client frame and nothing else, so the ids, numbers, member
//! order and terminator the server receives are the client's bytes. Parsing to a tree and writing
//! it back would turn an id above the 64-bit range into a different number.

use serde_json::Value;
use std::{io, ops::Range};

struct Member {
    key: String,
    /// From the opening quote of the key to the end of the value.
    whole: Range<usize>,
    value: Range<usize>,
}

fn skip_whitespace(bytes: &[u8], mut at: usize) -> usize {
    while bytes
        .get(at)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
    {
        at += 1;
    }
    at
}

fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut at = start + 1;
    loop {
        match bytes.get(at)? {
            b'\\' => at += 2,
            b'"' => return Some(at + 1),
            _ => at += 1,
        }
    }
}

fn value_end(bytes: &[u8], start: usize) -> Option<usize> {
    match bytes.get(start)? {
        b'"' => string_end(bytes, start),
        b'{' | b'[' => {
            let (mut at, mut depth) = (start, 0usize);
            loop {
                match bytes.get(at)? {
                    b'"' => {
                        at = string_end(bytes, at)?;
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return Some(at + 1);
                        }
                    }
                    _ => {}
                }
                at += 1;
            }
        }
        _ => {
            let mut at = start;
            while bytes.get(at).is_some_and(|byte| {
                !matches!(byte, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
            }) {
                at += 1;
            }
            Some(at)
        }
    }
}

/// The members of the object starting at `start`. The frame already parsed with unique members.
fn members(text: &str, start: usize) -> Option<Vec<Member>> {
    let bytes = text.as_bytes();
    let mut at = skip_whitespace(bytes, start);
    if bytes.get(at) != Some(&b'{') {
        return None;
    }
    at = skip_whitespace(bytes, at + 1);
    let mut found = Vec::new();
    if bytes.get(at) == Some(&b'}') {
        return Some(found);
    }
    loop {
        let key_end = string_end(bytes, at)?;
        let key = serde_json::from_str(text.get(at..key_end)?).ok()?;
        let colon = skip_whitespace(bytes, key_end);
        if bytes.get(colon) != Some(&b':') {
            return None;
        }
        let value_start = skip_whitespace(bytes, colon + 1);
        let end = value_end(bytes, value_start)?;
        found.push(Member {
            key,
            whole: at..end,
            value: value_start..end,
        });
        at = skip_whitespace(bytes, end);
        match bytes.get(at)? {
            b',' => at = skip_whitespace(bytes, at + 1),
            b'}' => return Some(found),
            _ => return None,
        }
    }
}

fn named(found: &[Member], key: &str) -> Option<Range<usize>> {
    found
        .iter()
        .find(|member| member.key == key)
        .map(|member| member.value.clone())
}

fn rewritten_arguments(text: &str, span: Range<usize>, effective: &Value) -> Option<String> {
    let found = members(text, span.start)?;
    let target = effective.as_object()?;
    let original: Value = serde_json::from_str(text.get(span.clone())?).ok()?;
    let changed = |member: &Member| original.get(&member.key) != target.get(&member.key);

    if found.iter().any(|member| !target.contains_key(&member.key)) {
        let kept: Vec<String> = found
            .iter()
            .filter_map(|member| {
                let value = target.get(&member.key)?;
                Some(if changed(member) {
                    format!("{}:{value}", Value::String(member.key.clone()),)
                } else {
                    text[member.whole.clone()].to_string()
                })
            })
            .collect();
        return Some(format!("{{{}}}", kept.join(",")));
    }

    let mut out = text.get(span.clone())?.to_string();
    for member in found.iter().rev().filter(|member| changed(member)) {
        let local = member.value.start - span.start..member.value.end - span.start;
        out.replace_range(local, &target.get(&member.key)?.to_string());
    }
    Some(out)
}

/// `line` with `params.arguments` holding `effective`, and every other byte kept.
pub(super) fn with_call_arguments(line: &str, effective: &Value) -> io::Result<String> {
    let rewrite = || {
        let params = named(&members(line, 0)?, "params")?;
        let arguments = named(&members(line, params.start)?, "arguments")?;
        let replacement = rewritten_arguments(line, arguments.clone(), effective)?;
        let mut out = line.to_string();
        out.replace_range(arguments, &replacement);
        let frame: Value = serde_json::from_str(out.trim_end()).ok()?;
        (frame["params"]["arguments"] == *effective).then_some(out)
    };
    rewrite().ok_or_else(|| io::Error::other("redacted frame could not be written"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn replaces_only_the_changed_member() {
        let line = "{\"id\":18446744073709551617,\"params\":{\"arguments\":{\"a\":1e2, \"b\":\"s\"},\"name\":\"t\"}}\r\n";
        let out = with_call_arguments(line, &json!({"a": 100.0, "b": "[R]"})).unwrap();
        assert_eq!(
            out,
            "{\"id\":18446744073709551617,\"params\":{\"arguments\":{\"a\":1e2, \"b\":\"[R]\"},\"name\":\"t\"}}\r\n"
        );
    }

    #[test]
    fn reads_a_key_spelled_with_an_escape() {
        let line = "{\"par\\u0061ms\":{\"\\u0061rguments\":{\"b\":\"}\\\"s\"}}}\n";
        let out = with_call_arguments(line, &json!({"b": "x"})).unwrap();
        assert_eq!(
            out,
            "{\"par\\u0061ms\":{\"\\u0061rguments\":{\"b\":\"x\"}}}\n"
        );
    }

    #[test]
    fn drops_a_member_and_empties_the_object() {
        let line = "{\"params\":{\"arguments\":{\"a\":1,\"b\":[{\"c\":\"]\"}]}}}\n";
        assert_eq!(
            with_call_arguments(line, &json!({"a": 1})).unwrap(),
            "{\"params\":{\"arguments\":{\"a\":1}}}\n"
        );
        assert_eq!(
            with_call_arguments(line, &json!({})).unwrap(),
            "{\"params\":{\"arguments\":{}}}\n"
        );
    }

    #[test]
    fn refuses_what_it_cannot_write() {
        let line = "{\"params\":{\"arguments\":{\"a\":1}}}\n";
        assert!(with_call_arguments(line, &json!({"a": 1, "new": 2})).is_err());
        assert!(with_call_arguments(line, &json!("text")).is_err());
        assert!(with_call_arguments("{\"params\":[1]}\n", &json!({})).is_err());
    }
}
