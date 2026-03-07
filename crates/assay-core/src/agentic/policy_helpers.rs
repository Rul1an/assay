use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PolicyShape {
    TopLevel, // allow/deny at root
    ToolsMap, // tools.allow/tools.deny
}

#[derive(Debug, Clone)]
pub(crate) struct PolicyCacheEntry {
    pub(crate) doc: serde_yaml::Value,
    pub(crate) shape: PolicyShape,
}

pub(crate) fn policy_pointers(shape: PolicyShape) -> (&'static str, &'static str) {
    match shape {
        PolicyShape::TopLevel => ("/allow", "/deny"),
        PolicyShape::ToolsMap => ("/tools/allow", "/tools/deny"),
    }
}

pub(crate) fn detect_policy_shape(doc: &serde_yaml::Value) -> PolicyShape {
    // Check if `tools` key exists and is a mapping
    let tools_map_opt = doc
        .as_mapping()
        .and_then(|m| m.get(serde_yaml::Value::String("tools".into())))
        .and_then(|v| v.as_mapping());

    if let Some(tm) = tools_map_opt {
        // Robust check: it's only the "ToolsMap" shape if allow/deny are SEQUENCES inside tools
        let has_allow = tm
            .get(serde_yaml::Value::String("allow".into()))
            .and_then(|v| v.as_sequence())
            .is_some();
        let has_deny = tm
            .get(serde_yaml::Value::String("deny".into()))
            .and_then(|v| v.as_sequence())
            .is_some();

        if has_allow || has_deny {
            return PolicyShape::ToolsMap;
        }
    }
    PolicyShape::TopLevel
}

pub(crate) fn read_yaml(path: &Path) -> Option<serde_yaml::Value> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_yaml::from_str::<serde_yaml::Value>(&s).ok()
}

pub(crate) fn get_policy_entry<'a>(
    cache: &'a mut BTreeMap<String, PolicyCacheEntry>,
    path_str: &str,
) -> Option<(&'a serde_yaml::Value, PolicyShape)> {
    if !cache.contains_key(path_str) {
        let pb = PathBuf::from(path_str);
        if let Some(doc) = read_yaml(&pb) {
            let shape = detect_policy_shape(&doc);
            cache.insert(path_str.to_string(), PolicyCacheEntry { doc, shape });
        }
    }
    cache.get(path_str).map(|e| (&e.doc, e.shape))
}

pub(crate) fn best_candidate(ctx: &serde_json::Value) -> Option<String> {
    // Prefer candidates[0] if present; else none.
    ctx.get("candidates")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// --- JSON Pointer helpers for YAML doc inspection (only for indexing remove ops) ---

pub(crate) fn get_seq_strings(doc: &serde_yaml::Value, ptr: &str) -> Option<Vec<String>> {
    let node = yaml_ptr(doc, ptr)?;
    let seq = node.as_sequence()?;
    let mut out = Vec::new();
    for it in seq {
        if let Some(s) = it.as_str() {
            out.push(s.to_string());
        }
    }
    Some(out)
}

pub(crate) fn find_in_seq(doc: &serde_yaml::Value, ptr: &str, target: &str) -> Option<usize> {
    let node = yaml_ptr(doc, ptr)?;
    let seq = node.as_sequence()?;
    for (i, it) in seq.iter().enumerate() {
        if it.as_str() == Some(target) {
            return Some(i);
        }
    }
    None
}

pub(crate) fn yaml_ptr<'a>(doc: &'a serde_yaml::Value, ptr: &str) -> Option<&'a serde_yaml::Value> {
    // special case: root
    if ptr.is_empty() || ptr == "/" {
        return Some(doc);
    }

    let mut cur = doc;
    for token in ptr.split('/').skip(1) {
        let key = unescape_pointer(token);
        match cur {
            serde_yaml::Value::Mapping(m) => {
                cur = m.get(serde_yaml::Value::String(key))?;
            }
            serde_yaml::Value::Sequence(seq) => {
                let idx: usize = key.parse().ok()?;
                cur = seq.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

pub(crate) fn escape_pointer(s: &str) -> String {
    // JSON Pointer escaping
    s.replace('~', "~0").replace('/', "~1")
}

pub(crate) fn unescape_pointer(s: &str) -> String {
    s.replace("~1", "/").replace("~0", "~")
}
