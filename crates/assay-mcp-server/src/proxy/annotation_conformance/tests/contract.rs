use super::*;

fn contract_fixture_path() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tool_annotation_conformance_contract.v0.json")
}

type ContractCase = (
    &'static str,
    ObservationBasis,
    DeclaredToolAnnotations,
    &'static str,
    Value,
    Option<&'static str>,
);

fn contract_records() -> Vec<Value> {
    let cases: Vec<ContractCase> = vec![
        (
            "consistent_read_only_false_additive",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: Some(false),
                destructive: None,
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app", "title": "ci-key"}),
            Some("sha256:tooldigest-consistent-readonly-false"),
        ),
        (
            "consistent_destructive_false_additive",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: Some(false),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app"}),
            Some("sha256:tooldigest-consistent-nondestructive"),
        ),
        (
            "mismatched_read_only_mutating",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: Some(true),
                destructive: None,
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app"}),
            Some("sha256:tooldigest-readonly-mismatch"),
        ),
        (
            "mismatched_non_destructive_destructive",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: Some(false),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            "workspace.modify_org_policy",
            json!({"workspace_id": "acme", "principal": "alice@example.com"}),
            Some("sha256:tooldigest-destructive-mismatch"),
        ),
        (
            "undeclared",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: None,
                destructive: None,
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app"}),
            Some("sha256:tooldigest-undeclared"),
        ),
        (
            "inconclusive_unknown_tool",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: Some(true),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            "unknown.tool",
            json!({}),
            Some("sha256:tooldigest-unknown"),
        ),
        (
            "unassessed_axes_recorded",
            ObservationBasis::Complete,
            DeclaredToolAnnotations {
                read_only: None,
                destructive: None,
                idempotent: Some(true),
                open_world: Some(false),
            },
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app"}),
            Some("sha256:tooldigest-unassessed"),
        ),
        (
            "incomplete_manifest_inconclusive",
            ObservationBasis::Incomplete,
            DeclaredToolAnnotations::default(),
            "github.add_deploy_key",
            json!({"owner": "acme", "repo": "prod-app"}),
            None,
        ),
    ];

    cases
        .iter()
        .map(|(case, basis, declared, tool, args, tool_digest)| {
            json!({
                "case": case,
                "record": build_tool_annotation_conformance_record(
                    *basis,
                    declared,
                    tool,
                    *tool_digest,
                    args
                ),
            })
        })
        .collect()
}

fn contract_document() -> Value {
    json!({
        "schema_contract": TOOL_ANNOTATION_CONFORMANCE_SCHEMA,
        "generated_by": "assay crates/assay-mcp-server proxy::annotation_conformance::build_tool_annotation_conformance_record (tool_annotation_conformance_contract_fixture)",
        "note": "Canonical producer output, regenerated from build_tool_annotation_conformance_record. Consumers vendor this file verbatim. Regenerate with ASSAY_UPDATE_GOLDEN=1.",
        "records": contract_records(),
    })
}

#[test]
fn tool_annotation_conformance_contract_fixture() {
    let generated = contract_document();
    let path = contract_fixture_path();

    if std::env::var("ASSAY_UPDATE_GOLDEN").is_ok() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let pretty = serde_json::to_string_pretty(&generated).unwrap();
        std::fs::write(&path, format!("{pretty}\n")).unwrap();
    }

    let committed_text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; regenerate with ASSAY_UPDATE_GOLDEN=1",
            path.display()
        )
    });
    let committed: Value = serde_json::from_str(&committed_text).unwrap();
    assert_eq!(
        committed, generated,
        "the committed tool-annotation conformance contract fixture is stale; regenerate with ASSAY_UPDATE_GOLDEN=1"
    );

    let records = generated["records"].as_array().unwrap();
    assert_eq!(records.len(), 8);
    for entry in records {
        let rec = &entry["record"];
        assert_eq!(rec["schema"], json!(TOOL_ANNOTATION_CONFORMANCE_SCHEMA));
        let obj = rec.as_object().unwrap();
        let mut keys: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "assessed_axes",
                "conformance",
                "declared",
                "mismatch_kind",
                "non_claims",
                "observation_basis",
                "observed",
                "schema",
                "tool",
                "unassessed_axes"
            ]
        );
        for forbidden in [
            "decision",
            "reason",
            "forwarded",
            "delivered",
            "credential_alias",
            "scopes",
            "target_digest",
            "caller_id",
        ] {
            assert!(
                rec.get(forbidden).is_none(),
                "carrier must not carry `{forbidden}`"
            );
        }
        let text = serde_json::to_string(rec).unwrap();
        for raw in ["ci-key", "alice@example.com"] {
            assert!(
                !text.contains(raw),
                "raw argument value {raw} must not appear in the contract fixture"
            );
        }
    }
}
