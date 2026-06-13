//! Pure producer contract for `assay.tool_annotation_conformance.v0` (Increment 5a).
//!
//! This carrier compares untrusted MCP ToolAnnotations against Assay's own rule-based call
//! classification. It is deliberately orthogonal to the enforcement verdict: a mismatch is a
//! conformance signal, never a deny, and a consistent record is not trust certification.
#![allow(dead_code)]

use assay_mcp_server::tool_decision::{classify, sanitize};
use serde_json::{json, Value};

pub const TOOL_ANNOTATION_CONFORMANCE_SCHEMA: &str = "assay.tool_annotation_conformance.v0";

#[derive(Debug, Clone, Copy, Default)]
pub struct DeclaredToolAnnotations {
    pub read_only: Option<bool>,
    pub destructive: Option<bool>,
    pub idempotent: Option<bool>,
    pub open_world: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObservedBehavior {
    Mutating,
    Destructive,
}

impl ObservedBehavior {
    fn as_str(self) -> &'static str {
        match self {
            ObservedBehavior::Mutating => "mutating",
            ObservedBehavior::Destructive => "destructive",
        }
    }
}

fn observed_behavior(verb: Option<&str>) -> Option<ObservedBehavior> {
    match verb {
        // Destructive means the call may overwrite/remove/reconfigure existing state. Additive admin
        // actions are still mutating, but they do not contradict `destructiveHint:false`.
        Some("change_role" | "modify") => Some(ObservedBehavior::Destructive),
        Some("create" | "add" | "grant" | "invite") => Some(ObservedBehavior::Mutating),
        _ => None,
    }
}

fn push_axis(axes: &mut Vec<&'static str>, axis: &'static str) {
    if !axes.contains(&axis) {
        axes.push(axis);
    }
}

/// Build one `assay.tool_annotation_conformance.v0` record from declared annotations and the
/// rule-based classification of the call. `tool_digest` is not available until the live observer
/// wiring slice, so 5a emits it as null while pinning the append-only field in the v0 shape.
pub fn conformance_for(declared: &DeclaredToolAnnotations, tool_name: &str, args: &Value) -> Value {
    build_tool_annotation_conformance_record(declared, tool_name, None, args)
}

pub fn build_tool_annotation_conformance_record(
    declared: &DeclaredToolAnnotations,
    tool_name: &str,
    tool_digest: Option<&str>,
    args: &Value,
) -> Value {
    let classified = classify(tool_name, args);
    let behavior = if classified.state == "classified" {
        observed_behavior(classified.verb)
    } else {
        None
    };

    let mut assessed_axes = Vec::new();
    let mut mismatch_kind: Option<&'static str> = None;

    if let Some(read_only) = declared.read_only {
        if behavior.is_some() {
            push_axis(&mut assessed_axes, "read_only");
            if read_only {
                mismatch_kind = Some("declared_read_only_observed_mutating");
            }
        }
    }

    if declared.read_only != Some(true) {
        if let Some(destructive) = declared.destructive {
            if let Some(observed) = behavior {
                push_axis(&mut assessed_axes, "destructive");
                if !destructive && observed == ObservedBehavior::Destructive {
                    mismatch_kind = Some("declared_non_destructive_observed_destructive");
                }
            }
        }
    }

    let conformance = if behavior.is_none() {
        "inconclusive"
    } else if assessed_axes.is_empty() {
        "undeclared"
    } else if mismatch_kind.is_some() {
        "mismatched"
    } else {
        "consistent"
    };

    let mut unassessed_axes = Vec::new();
    if declared.idempotent.is_some() {
        unassessed_axes.push("idempotent");
    }
    if declared.open_world.is_some() {
        unassessed_axes.push("open_world");
    }

    json!({
        "schema": TOOL_ANNOTATION_CONFORMANCE_SCHEMA,
        "tool": {
            "name": sanitize(tool_name),
            "tool_digest": tool_digest,
            "action_class": classified.category,
        },
        "declared": {
            "read_only": declared.read_only,
            "destructive": declared.destructive,
            "idempotent": declared.idempotent,
            "open_world": declared.open_world,
        },
        "observed": {
            "classification_state": classified.state,
            "verb": classified.verb,
            "behavior_class": behavior.map(ObservedBehavior::as_str),
        },
        "conformance": conformance,
        "mismatch_kind": mismatch_kind,
        "assessed_axes": assessed_axes,
        "unassessed_axes": unassessed_axes,
        "non_claims": [
            "tool annotations are untrusted hints, not security guarantees",
            "consistent does not certify the server or the annotation as trustworthy",
            "mismatch is a conformance signal, not a maliciousness verdict or an enforcement decision",
            "observed behavior is Assay's call classification, not verification of the upstream side effect",
            "idempotentHint and openWorldHint are recorded but not assessed in v0"
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn github_args() -> Value {
        json!({"owner": "acme", "repo": "prod-app", "title": "ci-key"})
    }

    fn workspace_args() -> Value {
        json!({"workspace_id": "acme", "principal": "alice@example.com"})
    }

    #[test]
    fn declared_read_only_true_mismatches_observed_mutating() {
        let rec = conformance_for(
            &DeclaredToolAnnotations {
                read_only: Some(true),
                destructive: None,
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            &github_args(),
        );

        assert_eq!(rec["schema"], json!(TOOL_ANNOTATION_CONFORMANCE_SCHEMA));
        assert_eq!(rec["conformance"], json!("mismatched"));
        assert_eq!(
            rec["mismatch_kind"],
            json!("declared_read_only_observed_mutating")
        );
        assert_eq!(rec["assessed_axes"], json!(["read_only"]));
    }

    #[test]
    fn declared_non_destructive_mismatches_observed_destructive() {
        let rec = conformance_for(
            &DeclaredToolAnnotations {
                read_only: Some(false),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            "workspace.modify_org_policy",
            &workspace_args(),
        );

        assert_eq!(rec["conformance"], json!("mismatched"));
        assert_eq!(
            rec["mismatch_kind"],
            json!("declared_non_destructive_observed_destructive")
        );
        assert_eq!(rec["observed"]["behavior_class"], json!("destructive"));
    }

    #[test]
    fn compatible_assessed_hints_are_consistent_not_certified() {
        let rec = conformance_for(
            &DeclaredToolAnnotations {
                read_only: Some(false),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            &github_args(),
        );

        assert_eq!(rec["conformance"], json!("consistent"));
        assert_eq!(rec["mismatch_kind"], Value::Null);
        assert_eq!(rec["observed"]["behavior_class"], json!("mutating"));
        assert!(rec["non_claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| { v.as_str().unwrap().contains("consistent does not certify") }));
    }

    #[test]
    fn missing_assessed_hints_are_undeclared_not_consistent() {
        let rec = conformance_for(
            &DeclaredToolAnnotations {
                read_only: None,
                destructive: None,
                idempotent: Some(true),
                open_world: Some(false),
            },
            "github.add_deploy_key",
            &github_args(),
        );

        assert_eq!(rec["conformance"], json!("undeclared"));
        assert_eq!(rec["assessed_axes"], json!([]));
        assert_eq!(rec["declared"]["idempotent"], json!(true));
        assert_eq!(rec["declared"]["open_world"], json!(false));
        assert_eq!(rec["unassessed_axes"], json!(["idempotent", "open_world"]));
    }

    #[test]
    fn unclassified_or_incomplete_calls_are_inconclusive() {
        for (tool, args) in [
            ("unknown.tool", json!({})),
            ("github.add_deploy_key", json!({"owner": "acme"})),
        ] {
            let rec = conformance_for(
                &DeclaredToolAnnotations {
                    read_only: Some(true),
                    destructive: Some(false),
                    idempotent: None,
                    open_world: None,
                },
                tool,
                &args,
            );

            assert_eq!(rec["conformance"], json!("inconclusive"));
            assert_eq!(rec["mismatch_kind"], Value::Null);
            assert_eq!(rec["assessed_axes"], json!([]));
        }
    }

    #[test]
    fn record_has_no_verdict_delivery_or_sensitive_identity_fields() {
        let rec = conformance_for(
            &DeclaredToolAnnotations {
                read_only: Some(false),
                destructive: Some(false),
                idempotent: None,
                open_world: None,
            },
            "github.add_deploy_key",
            &github_args(),
        );

        let text = serde_json::to_string(&rec).unwrap();
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
                "annotation conformance record must not carry field {forbidden}"
            );
        }
        assert!(
            !text.contains("ci-key"),
            "raw sensitive argument values must not be copied into the record"
        );
    }

    // ---- Shared producer/consumer contract fixture (Increment 5) ------------------------------
    //
    // The canonical `assay.tool_annotation_conformance.v0` contract is REAL output of
    // `build_tool_annotation_conformance_record`, not a hand-authored mirror. Plimsoll will vendor
    // the same file in a later slice and assert its consumer accepts each record.
    // Regenerate after an intentional producer change: ASSAY_UPDATE_GOLDEN=1 cargo test -p
    // assay-mcp-server tool_annotation_conformance_contract_fixture.

    fn contract_fixture_path() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/tool_annotation_conformance_contract.v0.json")
    }

    fn contract_records() -> Vec<Value> {
        let cases: &[(&str, DeclaredToolAnnotations, &str, Value, Option<&str>)] = &[
            (
                "consistent_read_only_false_additive",
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
        ];

        cases
            .iter()
            .map(|(case, declared, tool, args, tool_digest)| {
                json!({
                    "case": case,
                    "record": build_tool_annotation_conformance_record(
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
        assert_eq!(records.len(), 7);
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
}
