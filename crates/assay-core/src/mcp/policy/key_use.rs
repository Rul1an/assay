//! Which keys of a policy file `assay mcp wrap` acts on.
//!
//! The shared document reader refuses unknown keys. Wrap also checks which of the remaining
//! keys affect its decisions. Every key wrap does not act on is one of two kinds. Another
//! Assay reader acts on it, and one `policy.yaml` may serve them all, so wrap names that reader and
//! runs. Or no Assay reader acts on it: a misspelling, a key nothing reads, a key nobody knows. Wrap
//! refuses those ([`super::PolicySource::wrap_diagnostics`]), and so does `assay_check_args`, which decides as wrap
//! does.

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
    ("schemas", Acts),
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

impl super::PolicySource {
    /// Returns value-free preflight notes for an accepted policy, or `None` when wrap
    /// would refuse a key. The caller can then publish the fixed MCP refusal.
    pub fn check_args_notes(
        &self,
        policy: &super::McpPolicy,
        tool_name: &str,
    ) -> Option<Vec<String>> {
        let unread = unread_source(self.0.tree());
        if unread.iter().any(Unread::no_reader) {
            return None;
        }
        Some(describe_preflight(unread, policy, tool_name))
    }

    /// The keys wrap refuses, followed separately by the keys another reader acts on.
    pub fn wrap_diagnostics(&self) -> (Vec<String>, Vec<String>) {
        wrap_diagnostics_of(self.0.tree())
    }
}

fn wrap_diagnostics_of(source: &serde_yaml::Value) -> (Vec<String>, Vec<String>) {
    let mut refusals = Vec::new();
    let mut notes = Vec::new();
    for key in unread_source(source) {
        if key.no_reader() {
            refusals.push(key.describe());
        } else {
            notes.push(key.describe());
        }
    }
    (refusals, notes)
}

fn unread_source(source: &serde_yaml::Value) -> Vec<Unread> {
    let Some(root) = source.as_mapping() else {
        return Vec::new();
    };
    let mut unread = Vec::new();
    for (key, value) in root {
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
        if assay_common::doc::is_extension_key(key) {
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
    let (Some((_, members)), Some(map)) = (
        NESTED_KEYS.iter().find(|(name, _)| *name == parent),
        value.as_mapping(),
    ) else {
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

fn describe_preflight(
    unread: Vec<Unread>,
    policy: &super::McpPolicy,
    tool_name: &str,
) -> Vec<String> {
    let mut notes = Vec::new();
    let mut unknown = 0usize;
    for unread in unread {
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

    #[test]
    fn tagged_mappings_preserve_policy_reader_notes() {
        let plain = b"tools: {require_args: {}}";
        let (refusals, expected) = diagnostics(plain);
        assert!(refusals.is_empty());
        assert_eq!(expected.len(), 1);
        for tagged in [
            b"!policy {tools: {require_args: {}}}".as_slice(),
            b"tools: !settings {require_args: {}}",
        ] {
            assert_eq!(diagnostics(tagged), (Vec::new(), expected.clone()));
        }
    }

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
        // Absent allow lists are omitted when a policy is serialized.
        let tools = ToolPolicy {
            allow: Some(Vec::new()),
            allow_classes: Some(Vec::new()),
            ..Default::default()
        };
        assert_eq!(field_names(&tools), members("tools"));
        assert_eq!(field_names(&GlobalLimits::default()), members("limits"));
        assert_eq!(
            field_names(&EnforcementSettings::default()),
            members("enforcement")
        );
    }

    /// Wrap's diagnostics over a raw tree, so they are shown on keys and shapes the policy grammar
    /// refuses before a loaded source exists.
    fn diagnostics(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
        let value = super::super::legacy::yaml_value(bytes).unwrap();
        super::wrap_diagnostics_of(&value)
    }

    #[test]
    fn a_key_name_cannot_write_control_bytes_to_the_terminal() {
        let (notes, _) = diagnostics(b"\"evil\\e[2J\": 1\n");
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(!notes[0].contains('\u{1b}'), "{notes:?}");
    }

    #[test]
    fn a_file_that_is_not_a_mapping_has_no_notes() {
        assert_eq!(diagnostics(b"- a\n- b\n"), (vec![], vec![]));
        assert!(McpPolicy::from_slice_with_source(b"{").is_err());
    }

    #[test]
    fn a_key_set_to_null_is_not_named() {
        assert_eq!(
            diagnostics(b"signatures: null\nsequences: null\n"),
            (vec![], vec![])
        );
    }

    #[test]
    fn a_map_with_a_non_string_key_is_named() {
        assert_eq!(diagnostics(b"1: a\n").0.len(), 1);
    }
}
