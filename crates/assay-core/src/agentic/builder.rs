use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::errors::diagnostic::Diagnostic;

use super::policy_helpers::{
    best_candidate, escape_pointer, find_in_seq, get_policy_entry, get_seq_strings,
    policy_pointers, PolicyCacheEntry, PolicyShape,
};
use super::{AgenticCtx, JsonPatchOp, RiskLevel, SuggestedAction, SuggestedPatch};

pub(crate) fn build_suggestions_impl(
    diags: &[Diagnostic],
    ctx: &AgenticCtx,
) -> (Vec<SuggestedAction>, Vec<SuggestedPatch>) {
    // Deduplication maps
    let mut actions_map: BTreeMap<String, SuggestedAction> = BTreeMap::new();
    let mut patches_map: BTreeMap<String, SuggestedPatch> = BTreeMap::new();

    // Default policy path lookup
    let default_policy = ctx
        .policy_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("policy.yaml"));

    let default_config = ctx
        .config_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("assay.yaml"));

    // Policy docs are per-file; cache them (deterministic + avoids repeated IO).
    let mut policy_cache: BTreeMap<String, PolicyCacheEntry> = BTreeMap::new();

    for d in diags {
        // Resolve policy path for this specific diagnostic
        // Priority: 1. diag.context.policy_file, 2. ctx.policy_path, 3. "policy.yaml"
        let policy_path_str = d
            .context
            .get("policy_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_policy.display().to_string());

        // Resolve assay config path for this diagnostic
        // Priority: 1. diag.context.config_file, 2. ctx.config_path, 3. "assay.yaml"
        let config_path_str = d
            .context
            .get("config_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_config.display().to_string());

        // Load this policy doc (if available) and detect shape for pointers.
        let (policy_doc_this, policy_shape_this) =
            get_policy_entry(&mut policy_cache, &policy_path_str)
                .map(|(doc, shape)| (Some(doc), shape))
                .unwrap_or((None, PolicyShape::TopLevel));

        match d.code.as_str() {
            // ----------------------------
            // YAML parse errors -> actions
            // ----------------------------
            "E_CFG_PARSE" | "E_POLICY_PARSE" => {
                let id = "regen_config".to_string();
                actions_map.insert(
                    id.clone(),
                    SuggestedAction {
                        id,
                        title: "Regenerate a clean config (does not overwrite existing files)"
                            .into(),
                        risk: RiskLevel::Low,
                        command: vec!["assay".into(), "init".into()],
                    },
                );
            }

            // --------------------------------
            // Unknown field -> rename via move
            // --------------------------------
            "E_CFG_SCHEMA_UNKNOWN_FIELD" | "E_POLICY_SCHEMA_UNKNOWN_FIELD" => {
                let file = d.context.get("file").and_then(|v: &JsonValue| v.as_str());
                let parent = d
                    .context
                    .get("json_pointer_parent")
                    .and_then(|v: &JsonValue| v.as_str());
                let unknown = d
                    .context
                    .get("unknown_field")
                    .and_then(|v: &JsonValue| v.as_str());
                let suggested = d
                    .context
                    .get("suggested_field")
                    .and_then(|v: &JsonValue| v.as_str());

                if let (Some(file), Some(parent), Some(unknown), Some(suggested)) =
                    (file, parent, unknown, suggested)
                {
                    let id = format!("rename_field:{}->{}", unknown, suggested);
                    let from = format!(
                        "{}/{}",
                        parent.trim_end_matches('/'),
                        escape_pointer(unknown)
                    );
                    let to = format!(
                        "{}/{}",
                        parent.trim_end_matches('/'),
                        escape_pointer(suggested)
                    );

                    patches_map.insert(
                        id.clone(),
                        SuggestedPatch {
                            id,
                            title: format!("Rename field '{}' to '{}'", unknown, suggested),
                            risk: RiskLevel::Low,
                            file: file.to_string(),
                            ops: vec![JsonPatchOp::Move { from, path: to }],
                        },
                    );
                }
            }

            // --------------------------------------------------
            // Unknown tool -> Action only (no patch, safer)
            // --------------------------------------------------
            "UNKNOWN_TOOL" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let id = format!("fix_unknown_tool:{}", tool);
                    actions_map.insert(
                        id.clone(),
                        SuggestedAction {
                            id,
                            title: format!(
                                "Verify if tool '{}' exists and is named correctly in policy",
                                tool
                            ),
                            risk: RiskLevel::Low,
                            command: vec![
                                "assay".into(),
                                "doctor".into(),
                                "--format".into(),
                                "json".into(),
                            ],
                        },
                    );
                }
            }

            // --------------------------------------------------
            // Tool not allowed -> add to allowlist (Patch)
            // --------------------------------------------------
            "MCP_TOOL_NOT_ALLOWED" | "E_TOOL_NOT_ALLOWED" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let (allow_ptr, _) = policy_pointers(policy_shape_this);

                    // If allowlist is "*" then tool-not-allowed is likely not the issue.
                    let allow_is_wildcard = policy_doc_this
                        .and_then(|doc| get_seq_strings(doc, allow_ptr))
                        .map(|xs| xs.iter().any(|s| s == "*"))
                        .unwrap_or(false);

                    if !allow_is_wildcard {
                        let id = format!("allow_tool:{}", tool);
                        patches_map.insert(
                            id.clone(),
                            SuggestedPatch {
                                id,
                                title: format!("Allow tool '{}'", tool),
                                risk: RiskLevel::High,
                                file: policy_path_str.clone(),
                                ops: vec![JsonPatchOp::Add {
                                    path: format!("{}/-", allow_ptr),
                                    value: JsonValue::String(tool.to_string()),
                                }],
                            },
                        );
                    }
                }
            }

            // --------------------------------------------------
            // Tool denied -> remove from denylist (High risk)
            // --------------------------------------------------
            "E_EXEC_DENIED" | "MCP_TOOL_DENIED" | "E_TOOL_DENIED" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let (_, deny_ptr) = policy_pointers(policy_shape_this);

                    if let Some(doc) = policy_doc_this {
                        if let Some(idx) = find_in_seq(doc, deny_ptr, tool) {
                            let id = format!("remove_deny:{}", tool);
                            patches_map.insert(
                                id.clone(),
                                SuggestedPatch {
                                    id,
                                    title: format!("Remove '{}' from denylist", tool),
                                    risk: RiskLevel::High,
                                    file: policy_path_str.clone(),
                                    ops: vec![JsonPatchOp::Remove {
                                        path: format!("{}/{}", deny_ptr, idx),
                                    }],
                                },
                            );
                        } else {
                            let id = format!("manual_remove_deny:{}", tool);
                            actions_map.insert(
                                id.clone(),
                                SuggestedAction {
                                    id,
                                    title: format!(
                                        "Manually remove '{}' from denylist in {}",
                                        tool, policy_path_str
                                    ),
                                    risk: RiskLevel::High,
                                    command: vec![
                                        "assay".into(),
                                        "doctor".into(),
                                        "--format".into(),
                                        "json".into(),
                                    ],
                                },
                            );
                        }
                    }
                }
            }

            // --------------------------------------------------
            // Path scope/arg blocked -> add a constraint (Medium)
            // --------------------------------------------------
            "E_PATH_SCOPE_VIOLATION" | "E_ARG_PATTERN_BLOCKED" | "E_CONSTRAINT_MISSING" => {
                let tool = d
                    .context
                    .get("tool")
                    .and_then(|v: &JsonValue| v.as_str())
                    .unwrap_or("read_file");
                let param = d
                    .context
                    .get("param")
                    .and_then(|v: &JsonValue| v.as_str())
                    .unwrap_or("path");
                let re = d
                    .context
                    .get("recommended_matches")
                    .and_then(|v: &JsonValue| v.as_str())
                    .unwrap_or("^/app/.*|^/data/.*");

                let id = format!("add_constraint:{}:{}", tool, param);
                patches_map.insert(
                    id.clone(),
                    SuggestedPatch {
                        id,
                        title: format!("Add constraint {}.{} matches {}", tool, param, re),
                        risk: RiskLevel::Medium,
                        file: policy_path_str.clone(),
                        ops: vec![JsonPatchOp::Add {
                            path: "/constraints/-".into(),
                            value: serde_json::json!({
                                "tool": tool,
                                "params": {
                                    param: { "matches": re }
                                }
                            }),
                        }],
                    },
                );
            }

            // --------------------------------------------------
            // Tool poisoning -> Action only (Conservative for v1.5)
            // Avoids complex Replace/Add branching without EnsureObject
            // --------------------------------------------------
            "E_TOOL_POISONING_PATTERN" | "E_TOOL_DESC_SUSPICIOUS" | "E_SIGNATURES_DISABLED" => {
                let id = "enable_tool_poisoning_checks".to_string();
                actions_map.insert(
                    id.clone(),
                    SuggestedAction {
                        id,
                        title: format!(
                            "Enable tool poisoning heuristics (check_descriptions) in {}",
                            policy_path_str
                        ),
                        risk: RiskLevel::Low,
                        command: vec![
                            "assay".into(),
                            "fix".into(),
                            "--config".into(),
                            // IMPORTANT: `assay fix --config` points to assay.yaml (the config),
                            // not policy.yaml. The fixer can then follow `policy:` inside assay.yaml.
                            config_path_str.clone(),
                        ],
                    },
                );
            }

            // --------------------------------------------------
            // Missing paths in assay.yaml
            // --------------------------------------------------
            "E_PATH_NOT_FOUND" | "E_CFG_REF_MISSING" | "E_BASELINE_NOT_FOUND" => {
                let file = d
                    .context
                    .get("file")
                    .and_then(|v: &JsonValue| v.as_str())
                    .unwrap_or("assay.yaml");
                let field = d.context.get("field").and_then(|v: &JsonValue| v.as_str());

                if file.ends_with("assay.yaml") {
                    if let Some(field) = field {
                        if field == "policy" {
                            if let Some(best) = best_candidate(&d.context) {
                                let id = "fix_assay_policy_path".to_string();
                                patches_map.insert(
                                    id.clone(),
                                    SuggestedPatch {
                                        id,
                                        title: format!("Update assay.yaml policy path → {}", best),
                                        risk: RiskLevel::Low,
                                        file: file.to_string(),
                                        ops: vec![JsonPatchOp::Replace {
                                            path: "/policy".into(),
                                            value: JsonValue::String(best),
                                        }],
                                    },
                                );
                            }
                        }
                        if field == "baseline" {
                            let id = "fix_baseline_path".to_string();
                            patches_map.insert(
                                id.clone(),
                                SuggestedPatch {
                                    id,
                                    title: "Set baseline path to .assay/baseline.json".into(),
                                    risk: RiskLevel::Low,
                                    file: file.to_string(),
                                    ops: vec![JsonPatchOp::Replace {
                                        path: "/baseline".into(),
                                        value: JsonValue::String(".assay/baseline.json".into()),
                                    }],
                                },
                            );

                            let action_id = "create_baseline_dir".to_string();
                            actions_map.insert(
                                action_id.clone(),
                                SuggestedAction {
                                    id: action_id,
                                    title: "Create baseline directory".into(),
                                    risk: RiskLevel::Low,
                                    command: vec!["mkdir".into(), "-p".into(), ".assay".into()],
                                },
                            );
                        }
                    }
                }
            }

            // --------------------------------------------------
            // Trace drift -> action with context-aware filename
            // --------------------------------------------------
            "E_TRACE_SCHEMA_DRIFT" | "E_TRACE_SCHEMA_INVALID" | "E_TRACE_LEGACY_FUNCTION_CALL" => {
                let raw_trace_file = d
                    .context
                    .get("trace_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<raw.jsonl>");

                let id = "normalize_trace".to_string();
                actions_map.insert(
                    id.clone(),
                    SuggestedAction {
                        id,
                        title: "Normalize traces to Assay V2 schema".into(),
                        risk: RiskLevel::Low,
                        command: vec![
                            "assay".into(),
                            "trace".into(),
                            "ingest".into(),
                            "--input".into(),
                            raw_trace_file.to_string(),
                            "--output".into(),
                            "traces.jsonl".into(),
                        ],
                    },
                );
            }

            // --------------------------------------------------
            // Baseline mismatch
            // --------------------------------------------------
            "E_BASE_MISMATCH" | "E_BASELINE_SUITE_MISMATCH" => {
                let id = "export_baseline".to_string();
                actions_map.insert(
                    id.clone(),
                    SuggestedAction {
                        id,
                        title: "Export a new baseline from the current run".into(),
                        risk: RiskLevel::Low,
                        command: vec![
                            "assay".into(),
                            "run".into(),
                            "--export-baseline".into(),
                            ".assay/baseline.json".into(),
                        ],
                    },
                );
            }

            _ => {}
        }
    }

    // Convert BTreeMaps to Vecs (already sorted by id key)
    (
        actions_map.into_values().collect(),
        patches_map.into_values().collect(),
    )
}
