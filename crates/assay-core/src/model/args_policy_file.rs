//! Argument policy files project YAML into JSON for binding and migration. Object keys
//! have their authored string spelling, including keys that YAML would resolve as numbers.
use super::read_policy_text;
use assay_common::doc::{JsonInput, Opaque};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

assay_common::closed! {
    root struct ArgsDocument {
        version: Option<String>,
        name: Option<Opaque<Value>>,
        #[default]
        allow: Vec<String>,
        #[default]
        deny: Vec<String>,
        #[default]
        tools: ToolLists,
        #[default]
        enforcement: Enforcement,
        schemas: Option<Opaque<Value>>,
    }
}
assay_common::closed! {
    #[derive(Default)]
    struct ToolLists { #[default] allow: Vec<String>, #[default] deny: Vec<String> }
}
assay_common::closed! {
    struct Enforcement { #[default = "default_mode"] unconstrained_tools: String }
}

// The historical validator read each tool entry as a JSON Schema object and rejected
// calls to tools without an entry. Other policy controls need a manual migration.
assay_common::closed! {
    root struct LegacyArgsDocument {
        version: String,
        #[default]
        name: String,
        tools: LegacyTools,
    }
}
assay_common::closed! {
    struct LegacyTools { arg_constraints: Opaque<BTreeMap<String, BTreeMap<String, Value>>> }
}

fn default_mode() -> String {
    "warn".into()
}

impl Default for Enforcement {
    fn default() -> Self {
        Self {
            unconstrained_tools: default_mode(),
        }
    }
}

pub(super) struct StructuredControls {
    pub(super) allow: Vec<String>,
    pub(super) deny: Vec<String>,
    pub(super) schemas: Option<Value>,
    pub(super) deny_unconstrained: bool,
}

/// Project the declared fields into the values the evaluator consumes.
pub(super) fn read_structured_controls(policy: &Value) -> anyhow::Result<StructuredControls> {
    super::validation::validate_args_version(policy)?;
    if policy.get("schemas").is_some_and(|s| !s.is_object()) {
        anyhow::bail!("args_valid policy schemas must be a mapping");
    }
    let ArgsDocument {
        version: _version_label,
        name: _name_label,
        mut allow,
        mut deny,
        tools,
        enforcement,
        schemas,
    } = JsonInput::read_value::<ArgsDocument>(policy).map_err(|e| {
        super::validation::validate_inline_args_controls(policy)
            .err()
            .unwrap_or_else(|| super::policy_file::policy_refusal(e))
    })?;
    let ToolLists {
        allow: nested_allow,
        deny: nested_deny,
    } = tools;
    allow.extend(nested_allow);
    deny.extend(nested_deny);
    let Enforcement {
        unconstrained_tools,
    } = enforcement;
    Ok(StructuredControls {
        allow,
        deny,
        schemas: schemas.map(|value| value.0),
        deny_unconstrained: super::validation::unconstrained_mode_denies(Some(
            &unconstrained_tools,
        ))?,
    })
}

pub fn read_args_policy_file(path: &Path) -> anyhow::Result<Value> {
    read_args_file(path, false)
}

pub(crate) fn read_args_file_for_migration(path: &Path) -> anyhow::Result<Value> {
    read_args_file(path, true)
}

fn read_args_file(path: &Path, migrate: bool) -> anyhow::Result<Value> {
    let text = read_policy_text(path).map_err(|e| {
        anyhow::anyhow!("failed to read args_valid policy '{}': {e}", path.display())
    })?;
    let input = match JsonInput::parse(&text) {
        Ok(input) => input,
        Err(error) => {
            // Legacy string labels retain their YAML spelling even when the corresponding
            // JSON number cannot be represented. Only the complete legacy grammar may recover.
            if migrate {
                if let Ok(policy) = convert_legacy_args(&text) {
                    return Ok(policy);
                }
            }
            anyhow::bail!(
                "invalid args_valid policy YAML: {}",
                super::policy_file::policy_refusal(error)
            );
        }
    };
    if migrate && input.source().pointer("/tools/arg_constraints").is_some() {
        return convert_legacy_args(&text);
    }
    if super::has_structured_args_policy_shape(input.source()) {
        // The config binder and migrator need the original field presence and spellings.
        read_structured_controls(input.source())?;
        return Ok(input.into_source());
    }
    // On 6.x an unversioned map can still name a tool 'tools' or 'allow'.
    if !input.source().is_object() {
        anyhow::bail!("schema must be a non-empty tool-name-to-schema map");
    }
    input
        .into_checked_source::<BTreeMap<String, Opaque<Value>>>()
        .map_err(super::policy_file::policy_refusal)
}

fn convert_legacy_args(text: &str) -> anyhow::Result<Value> {
    let LegacyArgsDocument {
        version: _legacy_version_label,
        name,
        tools: LegacyTools { arg_constraints },
    } = assay_common::doc::load_with_string_keys(text)
        .map_err(super::policy_file::policy_refusal)?;
    Ok(serde_json::json!({
        "version": "2.0", "name": name, "schemas": arg_constraints.0,
        "enforcement": {"unconstrained_tools": "deny"}
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_common::unique_value::UniqueValue;

    #[test]
    fn unique_members_preserve_the_json_projection() {
        for source in [
            "",
            "# comment only",
            "0x10: {type: object}\n16: {type: string}",
            "schemas: {0x10: true, 16: false}",
            "version: '2'\n!key deny: [other]",
            "labels: [null, true, false, -1, 2.5, .inf, .nan, 18446744073709551615]",
            "!tag {deny: [other]}",
            "deny: !tag [other]",
            "bad: 170141183460469231731687303715884105727",
        ] {
            let old = serde_yaml::from_str::<Value>(source);
            let new = serde_yaml::from_str::<UniqueValue>(source).map(|value| value.0);
            match (old, new) {
                (Ok(expected), Ok(actual)) => assert_eq!(actual, expected, "{source}"),
                (Err(_), Err(_)) => {}
                (expected, actual) => panic!("{source}: expected {expected:?}, got {actual:?}"),
            }
        }
    }

    #[test]
    fn duplicate_members_are_refused_at_every_depth() {
        let err = serde_yaml::from_str::<UniqueValue>(
            "schemas: {read: {allOf: [{properties: {path: true, !key path: false}}]}}",
        )
        .expect_err("duplicate schema property was accepted");
        assert!(err.to_string().contains("duplicate member"), "{err}");
    }

    #[test]
    fn file_controls_reach_the_metric_with_the_authored_schema_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("args.yaml");
        std::fs::write(&path, "version: '2'\nname: null\nallow: [list]\ntools: {allow: [read], deny: [rm]}\nenforcement: {unconstrained_tools: deny}\nschemas: {0x10: true, 16: false}").unwrap();
        let source = read_args_policy_file(&path).unwrap();
        assert_eq!(source.get("name"), Some(&Value::Null));
        let super::super::ArgsPolicyView::Structured {
            allow,
            deny,
            deny_unconstrained,
            schemas,
        } = super::super::validated_args_policy(&source).unwrap()
        else {
            panic!("expected structured argument policy")
        };
        assert_eq!(allow, ["list", "read"]);
        assert_eq!(deny, ["rm"]);
        assert!(deny_unconstrained);
        assert_eq!(
            schemas,
            Some(serde_json::json!({"0x10": true, "16": false}))
        );
    }

    #[test]
    fn file_reader_refuses_unknown_controls_before_returning_the_policy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("args.yaml");
        std::fs::write(&path, "version: '2'\ntools: {deny: [rm], allows: [read]}").unwrap();
        let err = read_args_policy_file(&path).unwrap_err();
        assert!(err.to_string().contains("tools.allows"), "{err}");
    }
}
