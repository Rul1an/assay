//! Which keys of a policy file `assay mcp wrap` acts on.
//!
//! A policy loads with every key it does not understand ignored, so a control that does nothing
//! looks the same as one that works. This names the difference for whoever wrote the file.

use super::legacy::yaml_value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WrapUse {
    Acts,
    /// Names the file. Nothing about a call depends on it, and nobody expects it to.
    Label,
    Elsewhere(&'static str),
    Nowhere,
}

use WrapUse::{Acts, Elsewhere, Label, Nowhere};

const ROOT_KEYS: &[(&str, WrapUse)] = &[
    ("version", Label),
    ("name", Label),
    ("tools", Acts),
    ("allow", Acts),
    ("deny", Acts),
    ("schemas", Acts),
    ("constraints", Acts),
    ("enforcement", Acts),
    ("limits", Acts),
    ("tool_pins", Acts),
    ("tool_classes", Acts),
    ("signatures", Nowhere),
    ("discovery", Elsewhere("assay discover")),
    ("runtime_monitor", Elsewhere("assay monitor")),
    ("kill_switch", Elsewhere("assay monitor")),
];

/// Maps whose own keys a misspelling can hide in. Every member is read by wrap.
const NESTED_KEYS: &[(&str, &[&str])] = &[
    (
        "tools",
        &[
            "allow",
            "deny",
            "allow_classes",
            "deny_classes",
            "approval_required",
            "approval_required_classes",
            "restrict_scope",
            "restrict_scope_classes",
            "restrict_scope_contract",
            "redact_args",
            "redact_args_classes",
            "redact_args_contract",
        ],
    ),
    ("limits", &["max_requests_total", "max_tool_calls_total"]),
    ("enforcement", &["unconstrained_tools"]),
];

/// One line per key in this policy file that `assay mcp wrap` does not act on, in file order.
/// A file that is not a YAML mapping has none; the policy loader reports that.
pub fn keys_wrap_does_not_act_on(policy_file: &[u8]) -> Vec<String> {
    let Ok(serde_yaml::Value::Mapping(root)) = yaml_value(policy_file) else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    for (key, value) in &root {
        // A key set to null asks for nothing, whichever key it is.
        if value.is_null() {
            continue;
        }
        let Some(key) = key.as_str() else {
            notes.push("a policy key that is not a string is ignored".to_string());
            continue;
        };
        match ROOT_KEYS.iter().find(|(known, _)| *known == key) {
            None => notes.push(unknown_key(key)),
            Some((_, Nowhere)) => {
                notes.push(format!(
                    "policy key {key:?} has no effect: nothing reads it"
                ));
            }
            Some((_, Elsewhere(reader))) => notes.push(format!(
                "policy key {key:?} has no effect under assay mcp wrap; {reader} reads it"
            )),
            Some((_, Label)) => {}
            Some((_, Acts)) => nested_notes(key, value, &mut notes),
        }
    }
    notes
}

fn nested_notes(parent: &str, value: &serde_yaml::Value, notes: &mut Vec<String>) {
    let (Some((_, members)), serde_yaml::Value::Mapping(map)) =
        (NESTED_KEYS.iter().find(|(name, _)| *name == parent), value)
    else {
        return;
    };
    for key in map.keys().filter_map(serde_yaml::Value::as_str) {
        if !members.contains(&key) {
            notes.push(unknown_key(&format!("{parent}.{key}")));
        }
    }
}

fn unknown_key(key: &str) -> String {
    format!("{key:?} is not read by assay mcp wrap and is ignored")
}

#[cfg(test)]
mod tests {
    use super::super::{EnforcementSettings, GlobalLimits, McpPolicy, ToolPolicy};
    use super::*;
    use std::collections::BTreeSet;

    fn field_names<T: serde::Serialize>(value: &T) -> BTreeSet<String> {
        serde_json::to_value(value)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect()
    }

    fn sorted<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
        names.into_iter().map(str::to_string).collect()
    }

    /// A field added to the policy types must state here whether wrap acts on it.
    #[test]
    fn every_policy_field_has_a_row_and_every_row_a_field() {
        assert_eq!(
            field_names(&McpPolicy::default()),
            sorted(ROOT_KEYS.iter().map(|(key, _)| *key))
        );
    }

    #[test]
    fn every_nested_field_has_a_row_and_every_row_a_field() {
        let members = |parent: &str| {
            sorted(
                NESTED_KEYS
                    .iter()
                    .find(|(name, _)| *name == parent)
                    .unwrap()
                    .1
                    .iter()
                    .copied(),
            )
        };
        assert_eq!(field_names(&ToolPolicy::default()), members("tools"));
        assert_eq!(field_names(&GlobalLimits::default()), members("limits"));
        assert_eq!(
            field_names(&EnforcementSettings::default()),
            members("enforcement")
        );
    }

    #[test]
    fn a_key_name_cannot_write_control_bytes_to_the_terminal() {
        let notes = keys_wrap_does_not_act_on(b"\"evil\\e[2J\": 1\n");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(!notes[0].contains('\u{1b}'), "{notes:?}");
    }

    #[test]
    fn a_file_that_is_not_a_mapping_has_no_notes() {
        assert!(keys_wrap_does_not_act_on(b"- a\n- b\n").is_empty());
        assert!(keys_wrap_does_not_act_on(b"{").is_empty());
    }

    #[test]
    fn a_key_set_to_null_is_not_named() {
        assert!(keys_wrap_does_not_act_on(b"signatures: null\nsequences: null\n").is_empty());
    }

    #[test]
    fn a_map_with_a_non_string_key_is_named() {
        assert_eq!(keys_wrap_does_not_act_on(b"1: a\n").len(), 1);
    }
}
