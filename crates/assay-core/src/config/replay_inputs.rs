//! Replay's admission rule for the file references inside a config taken from a replay bundle.

use crate::errors::ConfigLoadError;
use crate::model::{EvalConfig, Expected};
use crate::render_safety::{render_safe, Sink, MAX_RENDER_FIELD};
use crate::replay::bundle::paths;
use std::collections::BTreeSet;
use std::path::{Component, Path};

/// Admit every file reference in a bundled config before anything reads it.
///
/// `sequence_valid.policy`, `args_valid.policy` (also beside an inline schema, where only the
/// fingerprint reads it) and `json_schema.schema_file` must each name, relative to the config's own
/// directory in the bundle, a canonical member listed in `members`. The reference is then rewritten
/// to that member under `workspace`, so the binder and the fingerprint read exactly the admitted
/// member. Anything else is refused with a config error before any read: a rooted or
/// drive-prefixed spelling, a `.`, `..` or empty segment, a backslash, or a name the bundle does
/// not list, also when a file of that name exists in the caller's directory.
pub(super) fn admit_replay_dependencies(
    cfg: &mut EvalConfig,
    config_path: &Path,
    workspace: &Path,
    members: &BTreeSet<String>,
) -> Result<(), ConfigLoadError> {
    let bundle_dir = config_bundle_dir(config_path, workspace).ok_or_else(|| {
        ConfigLoadError::new("replay config is not a member of the replay workspace")
    })?;
    for tc in &mut cfg.tests {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "only these variants carry a file reference"
        )]
        let (field, slot) = match &mut tc.expected {
            Expected::SequenceValid { policy, .. } => ("sequence_valid.policy", policy),
            Expected::ArgsValid { policy, .. } => ("args_valid.policy", policy),
            Expected::JsonSchema { schema_file, .. } => ("json_schema.schema_file", schema_file),
            _ => continue,
        };
        let Some(original) = slot.as_deref() else {
            continue;
        };
        let Some(member) = admitted_member(&bundle_dir, original, members) else {
            return Err(ConfigLoadError::new(format!(
                "replay refuses test '{}': {field} '{}' is not a member listed in the bundle inventory next to the config",
                render_safe(Sink::Json, &tc.id, MAX_RENDER_FIELD),
                render_safe(Sink::Json, original, MAX_RENDER_FIELD),
            )));
        };
        *slot = Some(workspace.join(&member).to_string_lossy().into_owned());
    }
    Ok(())
}

/// The config's directory inside the bundle as a member prefix (`files` for `files/eval.yaml`).
fn config_bundle_dir(config_path: &Path, workspace: &Path) -> Option<String> {
    let dir = config_path.strip_prefix(workspace).ok()?.parent()?;
    let segments = dir
        .components()
        .map(|c| match c {
            Component::Normal(segment) => segment.to_str(),
            Component::Prefix(_)
            | Component::RootDir
            | Component::CurDir
            | Component::ParentDir => None,
        })
        .collect::<Option<Vec<_>>>()?;
    Some(segments.join("/"))
}

/// The member a reference names, if it is admitted.
///
/// The spelling as written is checked first: once composed behind the bundle directory, the
/// entry-path rule's normalisation (leading `/` dropped, `\` read as `/`) could make a rooted or
/// drive-prefixed value look relative.
fn admitted_member(bundle_dir: &str, original: &str, members: &BTreeSet<String>) -> Option<String> {
    // A rooted value has an empty first segment; a drive-prefixed one has ':' in it.
    let segments: Vec<&str> = original.split(['/', '\\']).collect();
    if segments[0].contains(':')
        || segments
            .iter()
            .any(|segment| segment.is_empty() || *segment == "." || *segment == "..")
    {
        return None;
    }
    let member = format!("{bundle_dir}/{original}");
    paths::is_canonical_listed_member(&member, |m| members.contains(m)).then_some(member)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(expected: &str) -> EvalConfig {
        serde_yaml::from_str(&format!(
            "version: 1\nsuite: s\nmodel: dummy\ntests:\n  - id: t1\n    input: {{ prompt: \"hi\" }}\n    expected: {expected}\n"
        ))
        .expect("config")
    }

    fn members(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|m| m.to_string()).collect()
    }

    fn reference(cfg: &EvalConfig) -> Option<String> {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "the tests only build these variants"
        )]
        match &cfg.tests[0].expected {
            Expected::SequenceValid { policy, .. } | Expected::ArgsValid { policy, .. } => {
                policy.clone()
            }
            Expected::JsonSchema { schema_file, .. } => schema_file.clone(),
            _ => None,
        }
    }

    const FIELDS: [&str; 4] = [
        "{ type: sequence_valid, policy: \"REF\" }",
        "{ type: args_valid, policy: \"REF\" }",
        "{ type: args_valid, policy: \"REF\", schema: { version: \"2.0\", deny: [rm] } }",
        "{ type: json_schema, json_schema: \"\", schema_file: \"REF\" }",
    ];

    #[test]
    fn a_listed_member_next_to_the_config_is_admitted_and_rewritten() {
        let ws = Path::new("/ws");
        for field in FIELDS {
            for (reference_value, member) in [
                ("dep.yaml", "files/dep.yaml"),
                ("sub/dep.yaml", "files/sub/dep.yaml"),
            ] {
                let mut cfg = config(&field.replace("REF", reference_value));
                admit_replay_dependencies(
                    &mut cfg,
                    &ws.join("files/eval.yaml"),
                    ws,
                    &members(&[member, "files/eval.yaml"]),
                )
                .unwrap_or_else(|e| panic!("{field} {reference_value}: {e}"));
                assert_eq!(
                    reference(&cfg),
                    Some(ws.join(member).to_string_lossy().into_owned()),
                    "{field} {reference_value}"
                );
            }
        }
    }

    #[test]
    fn every_other_reference_is_refused() {
        let ws = Path::new("/ws");
        // Each refused value is listed in its composed or normalised form wherever that form could be
        // a member (`sub\dep.yaml` normalises to `files/sub/dep.yaml`), so a refusal here is the
        // spelling rule or canonical identity, not a missing listing.
        let listed = members(&[
            "files/eval.yaml",
            "files/C:/dep.yaml",
            "files/c:dep.yaml",
            "files/dep.yaml",
            "files/sub/dep.yaml",
        ]);
        for value in [
            "/etc/dep.yaml",
            "\\dep.yaml",
            "C:/dep.yaml",
            "C:\\dep.yaml",
            "c:dep.yaml",
            "../dep.yaml",
            "./dep.yaml",
            "sub/../dep.yaml",
            "sub//dep.yaml",
            "sub\\dep.yaml",
            "",
            "unlisted.yaml",
        ] {
            for field in FIELDS {
                let mut cfg = config(&field.replace("REF", &value.replace('\\', "\\\\")));
                let err =
                    admit_replay_dependencies(&mut cfg, &ws.join("files/eval.yaml"), ws, &listed)
                        .err()
                        .unwrap_or_else(|| panic!("{field} {value:?} was admitted"));
                assert!(
                    err.to_string().contains("replay refuses test 't1'"),
                    "{field} {value:?}: {err}"
                );
            }
        }
    }

    #[test]
    fn a_drive_prefixed_reference_is_refused_by_its_spelling_alone() {
        // `files/C:/dep.yaml` is a canonical entry path and listed here, so only the check on the
        // spelling as written refuses `C:/dep.yaml`.
        assert!(paths::is_canonical_listed_member(
            "files/C:/dep.yaml",
            |_| true
        ));
        assert_eq!(
            admitted_member("files", "C:/dep.yaml", &members(&["files/C:/dep.yaml"])),
            None
        );
    }

    #[test]
    fn variants_without_a_file_reference_and_inline_inputs_are_untouched() {
        let ws = Path::new("/ws");
        for expected in [
            "{ type: must_contain, must_contain: [ok] }",
            "{ type: sequence_valid, sequence: [a] }",
            "{ type: args_valid, schema: { version: \"2.0\", deny: [rm] } }",
            "{ type: json_schema, json_schema: \"{}\" }",
        ] {
            let mut cfg = config(expected);
            let before = serde_json::to_value(&cfg.tests[0].expected).expect("json");
            admit_replay_dependencies(&mut cfg, &ws.join("files/eval.yaml"), ws, &members(&[]))
                .unwrap_or_else(|e| panic!("{expected}: {e}"));
            assert_eq!(
                serde_json::to_value(&cfg.tests[0].expected).expect("json"),
                before,
                "{expected}"
            );
        }
    }
}
