use super::{compat, resolve};
use crate::lint::packs::loader::{load_pack, PackError, PackSource};
use crate::lint::packs::schema::PackValidationError;

#[test]
fn test_version_satisfies() {
    assert!(compat::version_satisfies_impl("2.9.0", "2.9.0"));
    assert!(compat::version_satisfies_impl("2.10.0", "2.9.0"));
    assert!(compat::version_satisfies_impl("3.0.0", "2.9.0"));
    assert!(!compat::version_satisfies_impl("2.8.0", "2.9.0"));
    assert!(!compat::version_satisfies_impl("2.9.0", "2.10.0"));
}

#[test]
fn test_levenshtein_distance() {
    assert_eq!(
        resolve::levenshtein_distance_impl("eu-ai-act", "eu-ai-act"),
        0
    );
    assert_eq!(
        resolve::levenshtein_distance_impl("eu-ai-act", "eu-ai-act-baseline"),
        9
    );
    assert_eq!(
        resolve::levenshtein_distance_impl("euaiact", "eu-ai-act"),
        2
    );
}

#[test]
fn test_is_valid_pack_name() {
    assert!(resolve::is_valid_pack_name_impl("simple"));
    assert!(resolve::is_valid_pack_name_impl("eu-ai-act-baseline"));
    assert!(resolve::is_valid_pack_name_impl("pack-v1"));
    assert!(resolve::is_valid_pack_name_impl("123-pack"));

    assert!(!resolve::is_valid_pack_name_impl(""));
    assert!(!resolve::is_valid_pack_name_impl("-start"));
    assert!(!resolve::is_valid_pack_name_impl("end-"));
    assert!(!resolve::is_valid_pack_name_impl("Caps"));
    assert!(!resolve::is_valid_pack_name_impl("dot.name"));
    assert!(!resolve::is_valid_pack_name_impl("space name"));
    assert!(!resolve::is_valid_pack_name_impl("/slash"));
}

// Mutex to serialize tests that modify environment variables
static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII Guard for test environment.
/// Sets XDG_CONFIG_HOME/APPDATA on creation, restores/clears on drop.
struct TestEnvGuard {
    _mutex_guard: std::sync::MutexGuard<'static, ()>,
    original_xdg: Option<String>,
    #[cfg(windows)]
    original_appdata: Option<String>,
}

impl TestEnvGuard {
    fn new(temp_dir: &tempfile::TempDir) -> Self {
        let guard = ENV_MUTEX.lock().unwrap();
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        #[cfg(windows)]
        let original_appdata = std::env::var("APPDATA").ok();

        let path = temp_dir.path();
        std::env::set_var("XDG_CONFIG_HOME", path);
        #[cfg(windows)]
        std::env::set_var("APPDATA", path);

        Self {
            _mutex_guard: guard,
            original_xdg,
            #[cfg(windows)]
            original_appdata,
        }
    }
}

impl Drop for TestEnvGuard {
    fn drop(&mut self) {
        match &self.original_xdg {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }

        #[cfg(windows)]
        match &self.original_appdata {
            Some(v) => std::env::set_var("APPDATA", v),
            None => std::env::remove_var("APPDATA"),
        }
    }
}

#[test]
fn test_local_pack_resolution() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let config_home = temp_dir.path();
    let packs_dir = config_home.join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let pack_content = r#"
name: local-pack
version: 1.0.0
kind: compliance
description: Test Local Pack
author: Me
license: MIT
disclaimer: Test disclaimer
requires:
  assay_min_version: "0.0.0"
rules:
  - id: LOC-001
    severity: info
    description: Local rule
    check:
      type: event_count
      min: 1
"#;
    std::fs::write(packs_dir.join("local-pack.yaml"), pack_content).unwrap();

    let dir_pack_dir = packs_dir.join("dir-pack");
    std::fs::create_dir_all(&dir_pack_dir).unwrap();
    let dir_pack_content = pack_content.replace("local-pack", "dir-pack");
    std::fs::write(dir_pack_dir.join("pack.yaml"), dir_pack_content).unwrap();

    let pack = load_pack("local-pack").expect("Should resolve local-pack");
    assert_eq!(pack.definition.name, "local-pack");
    assert!(matches!(pack.source, PackSource::File(_)));

    let pack = load_pack("dir-pack").expect("Should resolve dir-pack");
    assert_eq!(pack.definition.name, "dir-pack");
}

#[test]
fn test_builtin_wins_over_local() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let local_content = r#"
name: eu-ai-act-baseline
version: 9.9.9
kind: compliance
description: LOCAL SPOOF
author: Attacker
license: MIT
disclaimer: Spoof
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(packs_dir.join("eu-ai-act-baseline.yaml"), local_content).unwrap();

    let pack = load_pack("eu-ai-act-baseline").expect("Should load");
    match pack.source {
        PackSource::BuiltIn(_) => {}
        _ => panic!("Expected BuiltIn source, got {:?}", pack.source),
    }
    assert_ne!(pack.definition.description, "LOCAL SPOOF");
}

#[test]
fn test_local_resolves_name_dir_pack_yaml() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    let pack_dir = packs_dir.join("my-dir-pack");
    std::fs::create_dir_all(&pack_dir).unwrap();

    let content = r#"
name: my-dir-pack
version: 1.0.0
kind: compliance
description: Dir Pack
author: Me
license: MIT
disclaimer: Test
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(pack_dir.join("pack.yaml"), content).unwrap();

    let pack = load_pack("my-dir-pack").expect("Should resolve dir pack");
    assert_eq!(pack.definition.name, "my-dir-pack");
}

#[test]
fn test_local_invalid_yaml_fails() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();
    std::fs::write(packs_dir.join("broken.yaml"), ":: INVALID YAML ::").unwrap();

    let result = load_pack("broken");
    match result {
        Err(PackError::YamlParseError { .. }) => {}
        Ok(_) => panic!("Should have failed parsing"),
        Err(e) => panic!("Expected YamlParseError, got {:?}", e),
    }
}

#[test]
fn test_resolution_order_mock() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let packs_dir = temp_dir.path().join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let spoof_content = r#"
name: eu-ai-act-baseline
version: 9.9.9
kind: compliance
description: SPOOFED PACK
author: Attacker
license: MIT
disclaimer: Spoof disclaimer
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(packs_dir.join("eu-ai-act-baseline.yaml"), spoof_content).unwrap();

    let pack = load_pack("eu-ai-act-baseline").expect("Should load");
    match pack.source {
        PackSource::BuiltIn(_) => {}
        _ => panic!(
            "Should have loaded built-in pack, but got {:?}",
            pack.source
        ),
    }
    assert_ne!(pack.definition.description, "SPOOFED PACK");
}

#[test]
fn test_path_wins_over_builtin() {
    use tempfile::tempdir;
    let temp_dir = tempdir().unwrap();

    let pack_path = temp_dir.path().join("eu-ai-act-baseline.yaml");
    let override_content = r#"
name: eu-ai-act-baseline
version: 0.0.0
kind: compliance
description: OVERRIDE
author: Me
license: MIT
disclaimer: Override disclaimer
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(&pack_path, override_content).unwrap();

    let pack = load_pack(pack_path.to_str().unwrap()).expect("Should load by path");
    assert_eq!(pack.definition.description, "OVERRIDE");
}

#[test]
#[cfg(unix)]
fn test_symlink_escape_rejected() {
    use std::os::unix::fs::symlink;
    use tempfile::tempdir;

    let temp_dir = tempdir().unwrap();
    let _env_guard = TestEnvGuard::new(&temp_dir);

    let config_home = temp_dir.path();
    let outside_dir = temp_dir.path().join("outside");

    std::fs::create_dir_all(&outside_dir).unwrap();
    let packs_dir = config_home.join("assay").join("packs");
    std::fs::create_dir_all(&packs_dir).unwrap();

    let malicious_content = r#"
name: malicious
version: 1.0.0
kind: compliance
description: Evil
author: Hacker
license: MIT
disclaimer: Evil disclaimer
requires:
  assay_min_version: "0.0.0"
rules: []
"#;
    std::fs::write(outside_dir.join("malicious.yaml"), malicious_content).unwrap();

    symlink(
        outside_dir.join("malicious.yaml"),
        packs_dir.join("malicious.yaml"),
    )
    .unwrap();

    let result = load_pack("malicious");
    match result {
        Err(PackError::ValidationError(PackValidationError::Safety(msg))) => {
            assert!(msg.contains("resolves outside config directory"));
        }
        Err(e) => panic!("Expected Safety error, got: {:?}", e),
        Ok(_) => panic!("Should verified failed loading symlinked pack"),
    }
}

/// Rules, and the checks inside them, refuse fields the engine does not read. A misspelled rule key
/// used to vanish: `event_type` for `event_types` dropped the rule's scope, so it ran over every
/// event and the lint passed. Unknown check *types* are a different contract and stay a
/// forward-compatible `Unsupported`; so do the opaque JSON values a check carries.
mod strict_fields {
    use super::super::parse::load_pack_from_string_impl;
    use crate::lint::packs::loader::{LoadedPack, PackError, PackSource};
    use crate::lint::packs::schema::CheckDefinition;
    use crate::lint::packs::BUILTIN_PACKS;
    use std::path::PathBuf;

    /// A pack with one rule; `rule_extra` lines sit beside `check:`, `check` lines inside it.
    fn pack(kind: &str, rule_extra: &[&str], check: &[&str]) -> String {
        let mut s = format!(
            "name: strict-fields\nversion: \"1.0.0\"\nkind: {kind}\ndescription: strict field cells\n\
             author: test\nlicense: Apache-2.0\ndisclaimer: test only\nrequires:\n  assay_min_version: \">=3.3.0\"\n\
             rules:\n  - id: SF-001\n    severity: error\n    description: cell\n"
        );
        for line in rule_extra {
            s.push_str(&format!("    {line}\n"));
        }
        s.push_str("    check:\n");
        for line in check {
            s.push_str(&format!("      {line}\n"));
        }
        s
    }

    fn load(content: &str) -> Result<LoadedPack, PackError> {
        load_pack_from_string_impl(
            content,
            PackSource::File(PathBuf::from("strict-fields.yaml")),
        )
    }

    /// Each cell holds one unknown key, and the same pack without it loads.
    #[test]
    fn an_unknown_rule_or_check_field_refuses_the_pack() {
        let cells: &[(&str, &[&str], &[&str], &str)] = &[
            (
                "rule: event_type for event_types",
                &["event_type: [assay.never.emitted]"],
                &["type: event_count", "min: 1"],
                "event_type",
            ),
            (
                "rule: engine_min_versions",
                &["engine_min_versions: \"1.0\""],
                &["type: event_count", "min: 1"],
                "engine_min_versions",
            ),
            (
                "rule: help_markdwn",
                &["help_markdwn: text"],
                &["type: event_count", "min: 1"],
                "help_markdwn",
            ),
            (
                "event_count: maxx",
                &[],
                &["type: event_count", "min: 1", "maxx: 5"],
                "maxx",
            ),
            (
                "event_pairs: start_patterns",
                &[],
                &[
                    "type: event_pairs",
                    "start_pattern: a.*",
                    "finish_pattern: b.*",
                    "start_patterns: a.*",
                ],
                "start_patterns",
            ),
            (
                "event_field_present: path_any_of",
                &[],
                &[
                    "type: event_field_present",
                    "paths_any_of: [/data/x]",
                    "path_any_of: [/data/y]",
                ],
                "path_any_of",
            ),
            (
                "event_type_exists: patterns",
                &[],
                &[
                    "type: event_type_exists",
                    "pattern: assay.*",
                    "patterns: assay.*",
                ],
                "patterns",
            ),
            (
                "manifest_field: requires",
                &[],
                &["type: manifest_field", "path: /run_id", "requires: true"],
                "requires",
            ),
            (
                "json_path_exists: value_equal",
                &[],
                &[
                    "type: json_path_exists",
                    "paths: [/data/x]",
                    "value_equal: 1",
                ],
                "value_equal",
            ),
            (
                "conditional: conditions",
                &[],
                &[
                    "type: conditional",
                    "condition: {all: [{path: /data/x, equals: 1}]}",
                    "then: {type: json_path_exists, paths: [/data/y]}",
                    "conditions: {}",
                ],
                "conditions",
            ),
            (
                "g3 unit check: event_types misplaced under check",
                &[],
                &[
                    "type: g3_authorization_context_present",
                    "event_types: [assay.never.emitted]",
                ],
                "event_types",
            ),
            (
                "g3 unit check: any key",
                &[],
                &["type: g3_authorization_context_present", "strict: true"],
                "strict",
            ),
        ];
        let mut wrong = Vec::new();
        for (label, rule_extra, check, key) in cells {
            let content = pack("quality", rule_extra, check);
            match load(&content) {
                Err(PackError::YamlParseError { message }) => {
                    let named = message.contains(&format!("`{key}`"));
                    if !(message.contains("unknown field") && named)
                        || message.contains("not found")
                    {
                        wrong.push(format!(
                            "{label}: refused with an unexpected message: {message}"
                        ));
                    }
                }
                Err(other) => wrong.push(format!("{label}: refused for another reason: {other}")),
                Ok(_) => wrong.push(format!("{label}: loaded")),
            }
        }
        assert!(
            wrong.is_empty(),
            "unknown fields not refused:\n{}",
            wrong.join("\n")
        );
    }

    /// What strictness must not touch: unknown check types, opaque JSON values, legacy and
    /// defaulted fields, and the pack-level refusal that already existed.
    #[test]
    fn known_fields_unknown_types_and_opaque_values_still_load() {
        let accepted: &[(&str, &str, &[&str], &[&str])] = &[
            (
                "quality: unknown check type with its own fields",
                "quality",
                &[],
                &["type: event_cout", "min: 100"],
            ),
            (
                "compliance: unknown check type with its own fields",
                "compliance",
                &[],
                &["type: event_cout", "min: 100"],
            ),
            (
                "conditional: unknown keys inside the opaque condition and then",
                "quality",
                &[],
                &[
                    "type: conditional",
                    "condition: {all: [{path: /data/x, equals: 1}], note: free}",
                    "then: {type: json_path_exists, paths: [/data/y], extra: 1}",
                ],
            ),
            (
                "json_path_exists: object value_equals",
                "quality",
                &[],
                &[
                    "type: json_path_exists",
                    "paths: [/data/x]",
                    "value_equals: {any: {nested: [1, 2]}}",
                ],
            ),
            (
                "json_path_exists: null value_equals",
                "quality",
                &[],
                &[
                    "type: json_path_exists",
                    "paths: [/data/x]",
                    "value_equals: null",
                ],
            ),
            (
                "event_field_present: legacy any_of and in_data",
                "quality",
                &[],
                &[
                    "type: event_field_present",
                    "any_of: [tool]",
                    "in_data: true",
                ],
            ),
            (
                "g3 unit check alone",
                "quality",
                &[],
                &["type: g3_authorization_context_present"],
            ),
            (
                "rule: every documented optional field",
                "quality",
                &[
                    "article_ref: \"12(1)\"",
                    "help_markdown: text",
                    "engine_min_version: \"1.0\"",
                    "event_types: [assay.process.exec]",
                ],
                &["type: event_count", "min: 1"],
            ),
        ];
        let mut wrong = Vec::new();
        for (label, kind, rule_extra, check) in accepted {
            match load(&pack(kind, rule_extra, check)) {
                Ok(loaded) => {
                    let check = &loaded.definition.rules[0].check;
                    if label.contains("unknown check type")
                        && !matches!(check, CheckDefinition::Unsupported)
                    {
                        wrong.push(format!("{label}: loaded as {:?}, not Unsupported", check));
                    }
                }
                Err(e) => wrong.push(format!("{label}: refused: {e}")),
            }
        }
        let root_unknown = pack("quality", &[], &["type: event_count", "min: 1"])
            .replace("kind: quality\n", "kind: quality\nx-custom: 1\n");
        if !matches!(load(&root_unknown), Err(PackError::YamlParseError { .. })) {
            wrong.push("pack-level unknown field: no longer refused".to_string());
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// Every built-in pack loads with the digest and rule IDs it had before rule and check fields
    /// became strict (pins recorded on main `ccfbc567`).
    #[test]
    fn built_in_packs_keep_their_digest_and_rule_ids() {
        const PINS: &[(&str, &str, &[&str])] = &[
            (
                "cicd-starter",
                "sha256:11a648dad0f37781b296a5403c7ed2240cdece1531e05221b8aa39c3c7b06e1c",
                &["CICD-001", "CICD-002", "CICD-003", "CICD-004"],
            ),
            (
                "eu-ai-act-baseline",
                "sha256:97f10c43ae49e52261ee997ddcc7cc9a3c4a1f5c27d1403629fd27d24f1feb99",
                &["EU12-001", "EU12-002", "EU12-003", "EU12-004"],
            ),
            (
                "mandate-baseline",
                "sha256:74fe34930e0a1ff2321826ccbd93d86a3160c4ade9ccad96b865a32a5885e841",
                &[
                    "MANDATE-001",
                    "MANDATE-002",
                    "MANDATE-003",
                    "MANDATE-004",
                    "MANDATE-005",
                ],
            ),
            (
                "mcp-signal-followup",
                "sha256:be3196de38fec33228dcf484ab9dcc88c4bdc8ff6befe9efd42b74e25036dc0f",
                &["MCP-001", "MCP-002", "MCP-003"],
            ),
            (
                "a2a-signal-followup",
                "sha256:486311d90998a61cfc0d1a092b6f6ca997f8d3cd53cda91e9506a61a8b3eeba9",
                &["A2A-001", "A2A-002", "A2A-003"],
            ),
            (
                "a2a-discovery-card-followup",
                "sha256:48fbf21530f4d0109a8e9f421474da43d5c676c9299568030a61a7ddd29c767a",
                &["A2A-DC-001", "A2A-DC-002"],
            ),
            (
                "owasp-agentic-control-evidence-baseline",
                "sha256:9f26ea483fb507b6270263d943dd886ef30276fd18f5e3a6ac65d678ae5e41a9",
                &["A1-002", "A3-001", "A5-001"],
            ),
            (
                "owasp-agentic-a3-a5-signal-followup",
                "sha256:26107a5d37dc5c179c1d88d54b2e64aa1a70f4bfbf43e775f6f7e0e85cd7ec62",
                &["A3-003", "A5-002"],
            ),
            (
                "soc2-baseline",
                "sha256:e3de126dcb064f5aa0eb3ba768fdd9f96a831362ba9c0f0bb8a33539194a3d9e",
                &[
                    "invariant.accountability",
                    "invariant.change_lifecycle",
                    "invariant.decision_record",
                ],
            ),
        ];
        let mut actual = Vec::new();
        for (name, content) in BUILTIN_PACKS {
            let loaded = load_pack_from_string_impl(content, PackSource::BuiltIn(name))
                .unwrap_or_else(|e| panic!("built-in pack {name} no longer loads: {e}"));
            let ids: Vec<String> = loaded
                .definition
                .rules
                .iter()
                .map(|r| r.id.clone())
                .collect();
            actual.push((name.to_string(), loaded.digest.clone(), ids));
        }
        let rendered: Vec<String> = actual
            .iter()
            .map(|(n, d, ids)| {
                format!(
                    "(\"{n}\", \"{d}\", &[{}]),",
                    ids.iter()
                        .map(|i| format!("\"{i}\""))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            })
            .collect();
        assert_eq!(
            PINS.len(),
            actual.len(),
            "pins not recorded; actual:\n{}",
            rendered.join("\n")
        );
        for ((name, digest, ids), (pin_name, pin_digest, pin_ids)) in actual.iter().zip(PINS) {
            assert_eq!(
                (name.as_str(), digest.as_str()),
                (*pin_name, *pin_digest),
                "digest moved"
            );
            assert_eq!(
                ids.iter().map(String::as_str).collect::<Vec<_>>(),
                pin_ids.to_vec(),
                "rule IDs moved for {name}"
            );
        }
    }
}
