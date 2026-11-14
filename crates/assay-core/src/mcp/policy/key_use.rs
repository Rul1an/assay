//! Which keys of a policy file `assay mcp wrap` acts on.
//!
//! A policy loads with every key it does not understand ignored, so a control that does nothing
//! looks the same as one that works. Every key wrap does not act on is one of two kinds. Another
//! Assay reader acts on it, and one `policy.yaml` may serve them all, so wrap names that reader and
//! runs. Or no Assay reader acts on it: a misspelling, a key nothing reads, a key nobody knows. Wrap
//! refuses those ([`keys_wrap_refuses`]), and so does `assay_check_args`, which decides as wrap
//! does.

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

/// Root keys that another Assay policy reader acts on and `assay mcp wrap` does not, with that reader.
/// One `policy.yaml` may hold all of them (`docs/AIcontext/entry-points.md`).
const OTHER_TOOLS: &[(&str, &str)] = &[
    ("sequences", "assay_check_sequence"),
    ("aliases", "assay_check_sequence"),
    ("blocklist", "assay_policy_decide"),
];

/// Nested keys another Assay reader acts on and `assay mcp wrap` does not, with that reader. The
/// sequence policy's other fields under `tools` (`arg_constraints`) have no reader that acts on
/// them, so they are refused like any other unread key.
const OTHER_NESTED: &[(&str, &str)] = &[("tools.require_args", "assay coverage")];

/// A key `assay mcp wrap` does not act on. Only `Unknown` holds text from the file.
enum Unread {
    /// No Assay reader acts on it.
    NotString,
    /// No Assay reader knows it.
    Unknown(String),
    /// In the schema, and nothing reads it.
    Nowhere(&'static str),
    /// A root key another command acts on.
    Elsewhere(&'static str, &'static str),
    /// A root key another policy tool acts on.
    OtherTool(&'static str, &'static str),
}

impl Unread {
    /// The one rule: wrap refuses a key no Assay reader acts on, and names one another reader does.
    fn no_reader(&self) -> bool {
        match self {
            Unread::NotString | Unread::Unknown(_) | Unread::Nowhere(_) => true,
            Unread::Elsewhere(..) | Unread::OtherTool(..) => false,
        }
    }

    fn describe(&self) -> String {
        match self {
            Unread::NotString => {
                "a policy key that is not a string: no Assay reader acts on it".to_string()
            }
            Unread::Unknown(key) => format!("{key:?}: no Assay reader acts on it"),
            Unread::Nowhere(key) => format!("policy key {key:?} has no effect: nothing reads it"),
            Unread::Elsewhere(key, reader) | Unread::OtherTool(key, reader) => {
                format!("policy key {key:?} has no effect under assay mcp wrap; {reader} reads it")
            }
        }
    }
}

fn unread_keys(policy_file: &[u8]) -> Vec<Unread> {
    let Ok(serde_yaml::Value::Mapping(root)) = yaml_value(policy_file) else {
        return Vec::new();
    };
    let mut unread = Vec::new();
    for (key, value) in &root {
        // A key set to null asks for nothing, whichever key it is.
        if value.is_null() {
            continue;
        }
        let Some(key) = key.as_str() else {
            unread.push(Unread::NotString);
            continue;
        };
        // An extension field: a place for YAML anchors that the rest of the file refers to, read by
        // nobody on purpose (the convention of OpenAPI and Compose files). The eval config reader
        // lets them through for the same reason. Nothing is claimed by one, so nothing is said.
        if key.starts_with("x-") {
            continue;
        }
        match ROOT_KEYS.iter().find(|(known, _)| *known == key) {
            None => match OTHER_TOOLS.iter().find(|(name, _)| *name == key) {
                Some((name, reader)) => unread.push(Unread::OtherTool(name, reader)),
                None => unread.push(Unread::Unknown(key.to_string())),
            },
            Some((name, Nowhere)) => unread.push(Unread::Nowhere(name)),
            Some((name, Elsewhere(reader))) => unread.push(Unread::Elsewhere(name, reader)),
            Some((_, Label)) => {}
            Some((name, Acts)) => nested_unread(name, value, &mut unread),
        }
    }
    unread
}

fn nested_unread(parent: &str, value: &serde_yaml::Value, unread: &mut Vec<Unread>) {
    let (Some((_, members)), serde_yaml::Value::Mapping(map)) =
        (NESTED_KEYS.iter().find(|(name, _)| *name == parent), value)
    else {
        return;
    };
    for key in map.keys().filter_map(serde_yaml::Value::as_str) {
        if members.contains(&key) {
            continue;
        }
        let path = format!("{parent}.{key}");
        match OTHER_NESTED.iter().find(|(name, _)| *name == path) {
            Some((name, reader)) => unread.push(Unread::OtherTool(name, reader)),
            None => unread.push(Unread::Unknown(path)),
        }
    }
}

/// One line per key in this policy file that `assay mcp wrap` does not act on, in file order: the
/// lines of [`keys_wrap_refuses`] and of [`keys_another_reader_acts_on`] together. A file that is
/// not a YAML mapping has none; the policy loader reports that.
pub fn keys_wrap_does_not_act_on(policy_file: &[u8]) -> Vec<String> {
    unread_keys(policy_file)
        .iter()
        .map(Unread::describe)
        .collect()
}

/// One line per key in this policy file that no Assay reader acts on, in file order. `assay mcp
/// wrap` refuses a policy with any, and so does `assay_check_args`.
pub fn keys_wrap_refuses(policy_file: &[u8]) -> Vec<String> {
    unread_keys(policy_file)
        .iter()
        .filter(|unread| unread.no_reader())
        .map(Unread::describe)
        .collect()
}

/// One line per key in this policy file that another Assay reader acts on and `assay mcp wrap`
/// does not, naming that reader, in file order.
pub fn keys_another_reader_acts_on(policy_file: &[u8]) -> Vec<String> {
    unread_keys(policy_file)
        .iter()
        .filter(|unread| !unread.no_reader())
        .map(Unread::describe)
        .collect()
}

/// What `assay_check_args` does not weigh when it decides `tool_name` under this policy file.
///
/// The check decides a call as `assay mcp wrap` would before forwarding it
/// ([`super::McpPolicy::evaluate_preflight`]), so a key wrap does not act on is unread here too.
/// Two controls wrap acts on are out of reach: a pin on this tool needs the server's tool
/// definition, and `limits` count the calls of a session.
///
/// The notes go to an MCP client, so they use Assay's own key and reader names only. A key that no
/// Assay reader knows is counted, not quoted, and `assay policy validate` names it to the author.
pub fn preflight_notes(
    policy_file: &[u8],
    policy: &super::McpPolicy,
    tool_name: &str,
) -> Vec<String> {
    let mut notes = Vec::new();
    let mut unknown = 0usize;
    for unread in unread_keys(policy_file) {
        match unread {
            Unread::NotString => unknown += 1,
            Unread::Unknown(_) => unknown += 1,
            Unread::OtherTool(name, reader) => notes.push(format!(
                "policy key {name:?} is not read by assay_check_args; {reader} reads it"
            )),
            Unread::Nowhere(key) => notes.push(format!(
                "policy key {key:?} has no effect: nothing reads it"
            )),
            Unread::Elsewhere(key, reader) => notes.push(format!(
                "policy key {key:?} has no effect on this check; {reader} reads it"
            )),
        }
    }
    if unknown > 0 {
        let (verb, pronoun) = if unknown == 1 {
            ("is", "it")
        } else {
            ("are", "them")
        };
        let plural = if unknown == 1 { "" } else { "s" };
        notes.push(format!(
            "{unknown} policy key{plural} {verb} not read by assay_check_args or assay mcp wrap; \
             `assay policy validate` names {pronoun}"
        ));
    }
    if policy.tool_pins.contains_key(tool_name) {
        notes.push(
            "this tool has a pin in tool_pins; assay_check_args has no server tool definition to \
             compare it with, so assay mcp wrap decides the pin"
                .to_string(),
        );
    }
    if policy.limits.is_some() {
        notes.push(
            "limits count the calls of a session; assay_check_args does not count calls, so \
             assay mcp wrap decides them"
                .to_string(),
        );
    }
    notes
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
