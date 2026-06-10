//! P58a guard: the committed `assay.declared_tool_surface.v0` reference fixtures must hold the
//! structural invariants of the spec (docs/reference/declared-tool-surface.md). The diff and gate
//! (P58b) live in the downstream review layer; this only keeps the declared-side contract fixed.

use std::fs;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/declared_tool_surface")
}

const KNOWN_ACTION_CLASSES: &[&str] = &["github_deploy_key", "slack_add_member", "workspace_admin"];

#[test]
fn declared_tool_surface_fixtures_hold_the_spec_invariants() {
    let dir = fixtures_dir();
    let mut checked = 0;
    for entry in fs::read_dir(&dir).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap())
            .unwrap_or_else(|e| {
                panic!("fixture {} is not valid JSON: {e}", path.display());
            });
        let name = path.file_name().unwrap().to_string_lossy().to_string();

        assert_eq!(
            v["schema"].as_str(),
            Some("assay.declared_tool_surface.v0"),
            "{name}: wrong schema id"
        );
        let actions = v["declared_tool_actions"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: declared_tool_actions must be an array"));

        for a in actions {
            for key in ["id", "provider", "action_class", "required_decision"] {
                assert!(
                    a[key].as_str().map(|s| !s.is_empty()).unwrap_or(false),
                    "{name}: action field {key} must be a non-empty string"
                );
            }
            assert!(
                KNOWN_ACTION_CLASSES.contains(&a["action_class"].as_str().unwrap()),
                "{name}: unknown action_class {:?}",
                a["action_class"]
            );
            assert!(
                a["allowed_targets"].is_array(),
                "{name}: allowed_targets must be an array (possibly empty)"
            );
            let effects = a["allowed_effects"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: allowed_effects must be an array"));
            assert!(
                !effects.is_empty(),
                "{name}: allowed_effects must be non-empty"
            );
            for e in effects {
                let s = e.as_str().unwrap_or("");
                assert!(
                    s == "allow" || s == "deny",
                    "{name}: allowed_effects entries must be allow|deny, got {s:?}"
                );
            }
        }
        checked += 1;
    }
    assert!(
        checked >= 2,
        "expected both declared-surface fixtures, found {checked}"
    );
}
