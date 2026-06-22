//! Schema-aware set-path normalization.
//!
//! Given a record and the list of paths a schema registers as semantic *sets*, return a copy with
//! exactly those array fields sorted + deduped, and everything else untouched. This is the only
//! place array order is changed, and only for registered paths. Feeding the result to
//! [`crate::content_id`] (or [`crate::semantic_digest`]) yields a semantic-equivalence digest.
//!
//! Mirrors the reference contract (`tests/reference/canonical.py`): a registered value must be an
//! array of strings; anything else present at a registered path is malformed and rejected, never
//! coerced. A path that is simply absent from the record is skipped. Failures are typed
//! ([`SetPathError`]) so a fail-closed reject is auditable rather than a bare `None`.

use serde_json::{Map, Value};

/// A registered set path: the sequence of object keys leading to a set-valued array.
pub type SetPath = Vec<String>;

/// Why a record could not be set-normalized.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SetPathError {
    /// A registry entry had no keys; a set-path must name at least one field.
    #[error("empty set-path in registry")]
    EmptyPath,
    /// The value present at a registered set-path was not an array of strings.
    #[error("malformed set value at path {path:?}: expected an array of strings")]
    MalformedSetValue {
        /// The offending registered path.
        path: SetPath,
    },
}

/// Sort + dedupe a registered set value. `Some` for an array of strings; `None` for anything else
/// (not an array, or any non-string element) — malformed, reject rather than coerce.
fn canon_set(value: &Value) -> Option<Value> {
    let arr = value.as_array()?;
    let mut items: Vec<&str> = Vec::with_capacity(arr.len());
    for v in arr {
        items.push(v.as_str()?);
    }
    items.sort_unstable();
    items.dedup();
    Some(Value::Array(
        items
            .into_iter()
            .map(|s| Value::String(s.to_owned()))
            .collect(),
    ))
}

/// Walk `parents` (all but the last key) to the object that should hold the final key, or `None` if
/// any step is missing or not an object (path absent in this record).
fn parent_object<'a>(
    root: &'a mut Value,
    parents: &[String],
) -> Option<&'a mut Map<String, Value>> {
    let mut node = root;
    for key in parents {
        node = node.as_object_mut()?.get_mut(key)?;
    }
    node.as_object_mut()
}

/// Return a copy of `record` with each registered set-path normalized.
///
/// Errors with [`SetPathError::EmptyPath`] for an invalid registry entry, or
/// [`SetPathError::MalformedSetValue`] if a registered path is present but not an array of strings.
/// Absent paths are skipped; unregistered fields are left exactly as produced (order-significant).
pub fn normalize_sets(record: &Value, set_paths: &[SetPath]) -> Result<Value, SetPathError> {
    let mut out = record.clone();
    for path in set_paths {
        let (last, parents) = path.split_last().ok_or(SetPathError::EmptyPath)?;
        let Some(map) = parent_object(&mut out, parents) else {
            continue; // path absent -> nothing to normalize
        };
        match map.get(last) {
            None => continue, // path absent -> skip
            Some(target) => match canon_set(target) {
                Some(norm) => {
                    map.insert(last.clone(), norm);
                }
                None => {
                    return Err(SetPathError::MalformedSetValue { path: path.clone() });
                }
            },
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn p(keys: &[&str]) -> SetPath {
        keys.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn sorts_and_dedupes_registered_paths_only() {
        let rec = json!({"passed_keys": ["PATH", "HOME", "PATH"], "extends": ["b", "a"]});
        let out = normalize_sets(&rec, &[p(&["passed_keys"])]).unwrap();
        assert_eq!(out["passed_keys"], json!(["HOME", "PATH"])); // sorted + deduped
        assert_eq!(out["extends"], json!(["b", "a"])); // unregistered -> untouched
    }

    #[test]
    fn nested_path() {
        let rec = json!({"observed": {"tools": ["b", "a", "b"]}});
        let out = normalize_sets(&rec, &[p(&["observed", "tools"])]).unwrap();
        assert_eq!(out["observed"]["tools"], json!(["a", "b"]));
    }

    #[test]
    fn malformed_value_is_rejected_with_path() {
        assert_eq!(
            normalize_sets(&json!({"passed_keys": ["ok", 7]}), &[p(&["passed_keys"])]),
            Err(SetPathError::MalformedSetValue {
                path: p(&["passed_keys"])
            })
        );
        assert!(matches!(
            normalize_sets(
                &json!({"passed_keys": "not-a-list"}),
                &[p(&["passed_keys"])]
            ),
            Err(SetPathError::MalformedSetValue { .. })
        ));
    }

    #[test]
    fn empty_path_is_an_error() {
        assert_eq!(
            normalize_sets(&json!({}), &[vec![]]),
            Err(SetPathError::EmptyPath)
        );
    }

    #[test]
    fn absent_path_is_skipped() {
        let rec = json!({"other": 1});
        assert_eq!(normalize_sets(&rec, &[p(&["passed_keys"])]).unwrap(), rec);
    }

    #[test]
    fn empty_set_stays_empty() {
        let rec = json!({"network_endpoints": []});
        assert_eq!(
            normalize_sets(&rec, &[p(&["network_endpoints"])]).unwrap()["network_endpoints"],
            json!([])
        );
    }
}
