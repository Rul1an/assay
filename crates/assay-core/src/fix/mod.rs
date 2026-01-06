use anyhow::{anyhow, Context, Result};
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::path::Path;

use crate::agentic::JsonPatchOp;

/// Apply JSON Patch ops to a YAML or JSON document string.
/// - If `is_json == true`, parse as JSON and serialize as pretty JSON.
/// - Else parse as YAML and serialize as YAML.
///
/// NOTE: YAML formatting will be normalized by serde_yaml.
pub fn apply_ops_to_text(input: &str, ops: &[JsonPatchOp], is_json: bool) -> Result<String> {
    let mut doc: JsonValue = if is_json {
        serde_json::from_str(input).context("failed to parse JSON")?
    } else {
        let y: serde_yaml::Value = serde_yaml::from_str(input).context("failed to parse YAML")?;
        serde_json::to_value(y).context("failed to convert YAML->JSON")?
    };

    apply_ops_in_place(&mut doc, ops).context("failed to apply patch ops")?;

    if is_json {
        Ok(serde_json::to_string_pretty(&doc)?)
    } else {
        // Convert JSON back to YAML via Serialize
        let y = serde_yaml::to_value(&doc).context("failed to convert JSON->YAML")?;
        Ok(serde_yaml::to_string(&y)?)
    }
}

/// Apply JSON Patch ops to a file in-place using an atomic-ish write.
/// Returns the new content.
///
/// Notes:
/// - On Unix, `rename` overwrites atomically.
/// - On Windows, `rename` won’t overwrite; we remove destination first.
pub fn apply_ops_to_file(path: &Path, ops: &[JsonPatchOp]) -> Result<String> {
    use std::io::Write;

    let input = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let is_json = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let out = apply_ops_to_text(&input, ops, is_json)
        .with_context(|| format!("failed to patch {}", path.display()))?;

    let parent = path.parent().unwrap_or_else(|| Path::new("."));

    // Write to a tempfile in the same dir to keep rename semantics sane.
    let mut tmp = tempfile::Builder::new()
        .prefix(".assay_fix_")
        .tempfile_in(parent)
        .context("failed to create temp file")?;

    tmp.as_file_mut().write_all(out.as_bytes())?;
    let _ = tmp.as_file_mut().sync_all(); // best-effort

    // Persist to a deterministic sibling file first, then rename over destination.
    // This avoids Windows issues where persist-to-dest fails if dest exists.
    let tmp_path = {
        let fname = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("assay_tmp");
        parent.join(format!(".{}.assay_fix_tmp", fname))
    };

    let _ = std::fs::remove_file(&tmp_path);

    let _persisted = tmp
        .persist(&tmp_path)
        .map_err(|e| anyhow!("failed to persist temp file: {}", e))?;

    #[cfg(windows)]
    {
        let _ = std::fs::remove_file(path);
        std::fs::rename(&tmp_path, path).with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;
    }

    #[cfg(not(windows))]
    {
        std::fs::rename(&tmp_path, path).with_context(|| {
            format!(
                "failed to rename {} -> {}",
                tmp_path.display(),
                path.display()
            )
        })?;
    }

    Ok(out)
}

/// Apply a sequence of JSON Patch operations to a `serde_json::Value` in place.
pub fn apply_ops_in_place(doc: &mut JsonValue, ops: &[JsonPatchOp]) -> Result<()> {
    for op in ops {
        match op {
            JsonPatchOp::Add { path, value } => {
                add(doc, path, value.clone())?;
            }
            JsonPatchOp::Remove { path } => {
                remove(doc, path)?;
            }
            JsonPatchOp::Replace { path, value } => {
                replace(doc, path, value.clone())?;
            }
            JsonPatchOp::Move { from, path } => {
                let v = take(doc, from)?;
                add(doc, path, v)?;
            }
        }
    }
    Ok(())
}

// -----------------------
// JSON Pointer utilities
// -----------------------

fn parse_ptr(ptr: &str) -> Result<Vec<String>> {
    if ptr.is_empty() {
        return Ok(vec![]);
    }
    if !ptr.starts_with('/') {
        return Err(anyhow!("invalid JSON pointer (must start with /): {}", ptr));
    }
    Ok(ptr
        .trim_start_matches('/')
        .split('/')
        .map(unescape_ptr_token)
        .collect())
}

fn unescape_ptr_token(s: &str) -> String {
    s.replace("~1", "/").replace("~0", "~")
}

fn is_index_token(tok: &str) -> bool {
    tok == "-" || tok.parse::<usize>().is_ok()
}

fn type_name(v: &JsonValue) -> &'static str {
    match v {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "bool",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

/// Ensure the container for a child exists. Uses `next` token to decide array vs object.
/// This is ONLY appropriate for "add" / "create paths" behavior.
fn ensure_child_container<'a>(
    parent: &'a mut JsonValue,
    key: &str,
    next: Option<&str>,
) -> Result<&'a mut JsonValue> {
    let want_array = next.map(is_index_token).unwrap_or(false);

    match parent {
        JsonValue::Object(map) => {
            if !map.contains_key(key) || map.get(key).map(|v| v.is_null()).unwrap_or(false) {
                map.insert(
                    key.to_string(),
                    if want_array {
                        JsonValue::Array(vec![])
                    } else {
                        JsonValue::Object(JsonMap::new())
                    },
                );
            } else {
                // If it exists but is wrong type, overwrite (practical for fixes)
                let ok = if want_array {
                    map.get(key).map(|v| v.is_array()).unwrap_or(false)
                } else {
                    map.get(key).map(|v| v.is_object()).unwrap_or(false)
                };
                if !ok {
                    map.insert(
                        key.to_string(),
                        if want_array {
                            JsonValue::Array(vec![])
                        } else {
                            JsonValue::Object(JsonMap::new())
                        },
                    );
                }
            }
            Ok(map.get_mut(key).unwrap())
        }
        _ => Err(anyhow!(
            "expected object while ensuring path; got {}",
            type_name(parent)
        )),
    }
}

/// "Loose" traversal that creates missing containers. Use for add/building paths.
fn get_mut_loose<'a>(root: &'a mut JsonValue, tokens: &[String]) -> Result<&'a mut JsonValue> {
    let mut cur = root;
    for (i, tok) in tokens.iter().enumerate() {
        let next = tokens.get(i + 1).map(|s| s.as_str());

        match cur {
            JsonValue::Object(_) => {
                cur = ensure_child_container(cur, tok, next)?;
            }
            JsonValue::Array(arr) => {
                let idx: usize = tok
                    .parse()
                    .map_err(|_| anyhow!("expected array index, got '{}'", tok))?;
                if idx >= arr.len() {
                    return Err(anyhow!("index out of bounds while traversing: {}", tok));
                }
                cur = &mut arr[idx];
            }
            _ => return Err(anyhow!("cannot traverse into {}", type_name(cur))),
        }
    }
    Ok(cur)
}

/// Strict traversal that does NOT mutate the doc if the path is missing.
/// Use for remove/replace/take so we avoid partial mutation on failure.
fn get_mut_strict<'a>(root: &'a mut JsonValue, tokens: &[String]) -> Result<&'a mut JsonValue> {
    let mut cur = root;
    for tok in tokens {
        match cur {
            JsonValue::Object(map) => {
                cur = map
                    .get_mut(tok)
                    .ok_or_else(|| anyhow!("path does not exist at key '{}'", tok))?;
            }
            JsonValue::Array(arr) => {
                let idx: usize = tok
                    .parse()
                    .map_err(|_| anyhow!("expected array index, got '{}'", tok))?;
                cur = arr
                    .get_mut(idx)
                    .ok_or_else(|| anyhow!("index out of bounds: {}", idx))?;
            }
            _ => return Err(anyhow!("cannot traverse into {}", type_name(cur))),
        }
    }
    Ok(cur)
}

fn add(root: &mut JsonValue, ptr: &str, value: JsonValue) -> Result<()> {
    let tokens = parse_ptr(ptr)?;
    if tokens.is_empty() {
        *root = value;
        return Ok(());
    }

    let (parent_tokens, last) = tokens.split_at(tokens.len() - 1);
    let last = &last[0];

    let parent = get_mut_loose(root, parent_tokens)?;
    match parent {
        JsonValue::Object(map) => {
            if last == "-" {
                return Err(anyhow!("cannot add '-' key into object"));
            }
            map.insert(last.to_string(), value);
            Ok(())
        }
        JsonValue::Array(arr) => {
            if last == "-" {
                arr.push(value);
                Ok(())
            } else {
                let idx: usize = last
                    .parse()
                    .map_err(|_| anyhow!("expected array index, got '{}'", last))?;
                if idx > arr.len() {
                    return Err(anyhow!("add index out of bounds: {}", idx));
                }
                arr.insert(idx, value);
                Ok(())
            }
        }
        _ => Err(anyhow!(
            "add parent must be object/array, got {}",
            type_name(parent)
        )),
    }
}

fn replace(root: &mut JsonValue, ptr: &str, value: JsonValue) -> Result<()> {
    let tokens = parse_ptr(ptr)?;
    if tokens.is_empty() {
        *root = value;
        return Ok(());
    }

    let (parent_tokens, last) = tokens.split_at(tokens.len() - 1);
    let last = &last[0];

    let parent = get_mut_strict(root, parent_tokens)?;
    match parent {
        JsonValue::Object(map) => {
            if !map.contains_key(last) {
                return Err(anyhow!("replace target missing: {}", ptr));
            }
            map.insert(last.to_string(), value);
            Ok(())
        }
        JsonValue::Array(arr) => {
            let idx: usize = last
                .parse()
                .map_err(|_| anyhow!("expected array index, got '{}'", last))?;
            if idx >= arr.len() {
                return Err(anyhow!("replace index out of bounds: {}", idx));
            }
            arr[idx] = value;
            Ok(())
        }
        _ => Err(anyhow!(
            "replace parent must be object/array, got {}",
            type_name(parent)
        )),
    }
}

fn remove(root: &mut JsonValue, ptr: &str) -> Result<()> {
    let tokens = parse_ptr(ptr)?;
    if tokens.is_empty() {
        *root = JsonValue::Null;
        return Ok(());
    }

    let (parent_tokens, last) = tokens.split_at(tokens.len() - 1);
    let last = &last[0];

    let parent = get_mut_strict(root, parent_tokens)?;
    match parent {
        JsonValue::Object(map) => {
            map.remove(last)
                .ok_or_else(|| anyhow!("remove target missing: {}", ptr))?;
            Ok(())
        }
        JsonValue::Array(arr) => {
            let idx: usize = last
                .parse()
                .map_err(|_| anyhow!("expected array index, got '{}'", last))?;
            if idx >= arr.len() {
                return Err(anyhow!("remove index out of bounds: {}", idx));
            }
            arr.remove(idx);
            Ok(())
        }
        _ => Err(anyhow!(
            "remove parent must be object/array, got {}",
            type_name(parent)
        )),
    }
}

fn take(root: &mut JsonValue, ptr: &str) -> Result<JsonValue> {
    let tokens = parse_ptr(ptr)?;
    if tokens.is_empty() {
        let mut tmp = JsonValue::Null;
        std::mem::swap(&mut tmp, root);
        return Ok(tmp);
    }

    let (parent_tokens, last) = tokens.split_at(tokens.len() - 1);
    let last = &last[0];

    let parent = get_mut_strict(root, parent_tokens)?;
    match parent {
        JsonValue::Object(map) => map
            .remove(last)
            .ok_or_else(|| anyhow!("move/from missing: {}", ptr)),
        JsonValue::Array(arr) => {
            let idx: usize = last
                .parse()
                .map_err(|_| anyhow!("expected array index, got '{}'", last))?;
            if idx >= arr.len() {
                return Err(anyhow!("move/from index out of bounds: {}", idx));
            }
            Ok(arr.remove(idx))
        }
        _ => Err(anyhow!(
            "move/from parent must be object/array, got {}",
            type_name(parent)
        )),
    }
}

#[cfg(test)]
fn escape_ptr_token(s: &str) -> String {
    s.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_escape_unescape_pointer() {
        // Test cases from RFC 6901
        let cases = vec![
            ("~", "~0"),
            ("/", "~1"),
            ("a/b", "a~1b"),
            ("m~n", "m~0n"),
            ("~/", "~0~1"),
            ("/~", "~1~0"),
            ("foo/bar~baz", "foo~1bar~0baz"),
        ];

        for (plain, escaped) in cases {
            assert_eq!(escape_ptr_token(plain), escaped, "Escaping '{}'", plain);
            assert_eq!(
                unescape_ptr_token(escaped),
                plain,
                "Unescaping '{}'",
                escaped
            );
        }
    }

    #[test]
    fn test_apply_patch_memory_json() {
        let input = r#"{ "foo": "bar" }"#;
        let ops = vec![
            JsonPatchOp::Replace {
                path: "/foo".into(),
                value: json!("baz"),
            },
            JsonPatchOp::Add {
                path: "/new".into(),
                value: json!(123),
            },
        ];

        let out = apply_ops_to_text(input, &ops, true).expect("apply success");
        let parsed: JsonValue = serde_json::from_str(&out).unwrap();

        assert_eq!(parsed["foo"], "baz");
        assert_eq!(parsed["new"], 123);
    }

    #[test]
    fn test_remove_strict_does_not_create_paths() {
        let mut doc = json!({"a": {"b": 1}});
        let ops = vec![JsonPatchOp::Remove {
            path: "/a/missing".into(),
        }];

        let err = apply_ops_in_place(&mut doc, &ops).unwrap_err();
        assert!(err.to_string().contains("remove target missing"));
        // Ensure original doc not mutated
        assert_eq!(doc, json!({"a": {"b": 1}}));
    }
}
