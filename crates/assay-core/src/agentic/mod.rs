// crates/assay-core/src/agentic/mod.rs
// A reusable "Agentic Contract" builder that turns Diagnostics into:
// - suggested_actions (commands to run)
// - suggested_patches (JSON Patch ops, machine-applyable)
//
// This is intentionally conservative + deterministic.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::path::{Path, PathBuf};

use crate::errors::diagnostic::Diagnostic;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedAction {
    pub id: String,
    pub title: String,
    pub risk: RiskLevel,
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedPatch {
    pub id: String,
    pub title: String,
    pub risk: RiskLevel,
    pub file: String, // path relative to cwd (or absolute)
    pub ops: Vec<JsonPatchOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonPatchOp {
    Add { path: String, value: JsonValue },
    Remove { path: String },
    Replace { path: String, value: JsonValue },
    Move { from: String, path: String },
}

#[derive(Debug, Clone, Copy)]
enum PolicyShape {
    TopLevel, // allow/deny at root
    ToolsMap, // tools.allow/tools.deny
}

pub struct AgenticCtx {
    /// If you can infer this from assay.yaml, pass it in.
    pub policy_path: Option<PathBuf>,
}

/// Main entrypoint: build suggestions for any diagnostics list.
pub fn build_suggestions(
    diags: &[Diagnostic],
    ctx: &AgenticCtx,
) -> (Vec<SuggestedAction>, Vec<SuggestedPatch>) {
    let mut actions: Vec<SuggestedAction> = Vec::new();
    let mut patches: Vec<SuggestedPatch> = Vec::new();

    // If we need to compute deny index etc, we may need the parsed policy file.
    let policy_path = ctx
        .policy_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("policy.yaml"));
    let policy_doc = read_yaml(&policy_path);
    let policy_shape = policy_doc
        .as_ref()
        .map(|doc| detect_policy_shape(doc))
        .unwrap_or(PolicyShape::TopLevel);

    for d in diags {
        match d.code.as_str() {
            // ----------------------------
            // YAML parse errors -> actions
            // ----------------------------
            "E_CFG_PARSE" | "E_POLICY_PARSE" => {
                actions.push(SuggestedAction {
                    id: "regen_config".into(),
                    title: "Regenerate a clean config (does not overwrite existing files)".into(),
                    risk: RiskLevel::Low,
                    command: vec!["assay".into(), "init".into()],
                });
            }

            // --------------------------------
            // Unknown field -> rename via move
            // Requires context fields:
            //  - file
            //  - json_pointer_parent
            //  - unknown_field
            //  - suggested_field
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
                    // JSON Patch "move" is perfect for rename without cloning values.
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

                    patches.push(SuggestedPatch {
                        id: format!("rename_field:{}->{}", unknown, suggested),
                        title: format!("Rename field '{}' to '{}'", unknown, suggested),
                        risk: RiskLevel::Low,
                        file: file.to_string(),
                        ops: vec![JsonPatchOp::Move { from, path: to }],
                    });
                }
            }

            // --------------------------------------------------
            // Tool not allowed -> add to allowlist
            // Requires context:
            //  - tool (string)
            // --------------------------------------------------
            "MCP_TOOL_NOT_ALLOWED" | "E_TOOL_NOT_ALLOWED" | "UNKNOWN_TOOL" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let (allow_ptr, _) = policy_pointers(policy_shape);

                    // If allowlist is "*" then tool-not-allowed is likely not the issue.
                    // We'll still propose patch, but only if allowlist isn't wildcard.
                    let allow_is_wildcard = policy_doc
                        .as_ref()
                        .and_then(|doc| get_seq_strings(doc, allow_ptr))
                        .map(|xs| xs.iter().any(|s| s == "*"))
                        .unwrap_or(false);

                    if !allow_is_wildcard {
                        patches.push(SuggestedPatch {
                            id: format!("allow_tool:{}", tool),
                            title: format!("Allow tool '{}'", tool),
                            risk: RiskLevel::High,
                            file: policy_path.display().to_string(),
                            ops: vec![JsonPatchOp::Add {
                                path: format!("{}/-", allow_ptr),
                                value: JsonValue::String(tool.to_string()),
                            }],
                        });
                    }
                }
            }

            // --------------------------------------------------
            // Tool denied -> remove from denylist (high risk)
            // Requires context:
            //  - tool (string)
            // --------------------------------------------------
            "E_EXEC_DENIED" | "MCP_TOOL_DENIED" | "E_TOOL_DENIED" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let (_, deny_ptr) = policy_pointers(policy_shape);

                    if let Some(doc) = policy_doc.as_ref() {
                        if let Some(idx) = find_in_seq(doc, deny_ptr, tool) {
                            patches.push(SuggestedPatch {
                                id: format!("remove_deny:{}", tool),
                                title: format!("Remove '{}' from denylist", tool),
                                risk: RiskLevel::High,
                                file: policy_path.display().to_string(),
                                ops: vec![JsonPatchOp::Remove {
                                    path: format!("{}/{}", deny_ptr, idx),
                                }],
                            });
                        } else {
                            // Can't find index deterministically -> suggest action instead
                            actions.push(SuggestedAction {
                                id: format!("manual_remove_deny:{}", tool),
                                title: format!(
                                    "Manually remove '{}' from denylist in {}",
                                    tool,
                                    policy_path.display()
                                ),
                                risk: RiskLevel::High,
                                command: vec![
                                    "assay".into(),
                                    "doctor".into(),
                                    "--format".into(),
                                    "json".into(),
                                ],
                            });
                        }
                    }
                }
            }

            // --------------------------------------------------
            // Path scope/arg blocked -> add a constraint (medium)
            // Requires context (preferably):
            //  - tool
            //  - param
            //  - recommended_matches (regex)
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

                // Your current template uses constraints as a LIST at "/constraints".
                patches.push(SuggestedPatch {
                    id: format!("add_constraint:{}:{}", tool, param),
                    title: format!("Add constraint {}.{} matches {}", tool, param, re),
                    risk: RiskLevel::Medium,
                    file: policy_path.display().to_string(),
                    ops: vec![JsonPatchOp::Add {
                        path: "/constraints/-".into(),
                        value: serde_json::json!({
                            "tool": tool,
                            "params": {
                                param: { "matches": re }
                            }
                        }),
                    }],
                });
            }

            // --------------------------------------------------
            // Tool poisoning checks -> enable signatures.check_descriptions (low)
            // --------------------------------------------------
            "E_TOOL_POISONING_PATTERN" | "E_TOOL_DESC_SUSPICIOUS" | "E_SIGNATURES_DISABLED" => {
                // If /signatures doesn't exist, add it. If it exists, replace check_descriptions=true.
                // We do not read YAML here; we can express as 2 alternative patches.
                // For v1.5, emit one conservative patch: "replace" (will fail if missing),
                // plus an action to run "assay fix" (which can do conditional apply).
                patches.push(SuggestedPatch {
                    id: "enable_tool_poisoning_checks".into(),
                    title: "Enable tool poisoning heuristics (check tool descriptions)".into(),
                    risk: RiskLevel::Low,
                    file: policy_path.display().to_string(),
                    ops: vec![
                        JsonPatchOp::Add {
                            path: "/signatures".into(),
                            value: serde_json::json!({ "check_descriptions": true }),
                        },
                        JsonPatchOp::Replace {
                            path: "/signatures/check_descriptions".into(),
                            value: JsonValue::Bool(true),
                        },
                    ],
                });
            }

            // --------------------------------------------------
            // Missing paths in assay.yaml -> fix policy/baseline pointers (low)
            // Requires context:
            //  - file, field, candidates[]
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
                                patches.push(SuggestedPatch {
                                    id: "fix_assay_policy_path".into(),
                                    title: format!("Update assay.yaml policy path → {}", best),
                                    risk: RiskLevel::Low,
                                    file: file.to_string(),
                                    ops: vec![JsonPatchOp::Replace {
                                        path: "/policy".into(),
                                        value: JsonValue::String(best),
                                    }],
                                });
                            }
                        }
                        if field == "baseline" {
                            patches.push(SuggestedPatch {
                                id: "fix_baseline_path".into(),
                                title: "Set baseline path to .assay/baseline.json".into(),
                                risk: RiskLevel::Low,
                                file: file.to_string(),
                                ops: vec![JsonPatchOp::Replace {
                                    path: "/baseline".into(),
                                    value: JsonValue::String(".assay/baseline.json".into()),
                                }],
                            });

                            actions.push(SuggestedAction {
                                id: "create_baseline_dir".into(),
                                title: "Create baseline directory".into(),
                                risk: RiskLevel::Low,
                                command: vec!["mkdir".into(), "-p".into(), ".assay".into()],
                            });
                        }
                    }
                }
            }

            // --------------------------------------------------
            // Trace drift -> action only in v1.5
            // --------------------------------------------------
            "E_TRACE_SCHEMA_DRIFT" | "E_TRACE_SCHEMA_INVALID" | "E_TRACE_LEGACY_FUNCTION_CALL" => {
                actions.push(SuggestedAction {
                    id: "normalize_trace".into(),
                    title: "Normalize traces to Assay V2 schema".into(),
                    risk: RiskLevel::Low,
                    command: vec![
                        "assay".into(),
                        "trace".into(),
                        "ingest".into(),
                        "--input".into(),
                        "<raw.jsonl>".into(),
                        "--output".into(),
                        "traces.jsonl".into(),
                    ],
                });
            }

            // --------------------------------------------------
            // Baseline mismatch -> action only
            // --------------------------------------------------
            "E_BASE_MISMATCH" | "E_BASELINE_SUITE_MISMATCH" => {
                actions.push(SuggestedAction {
                    id: "export_baseline".into(),
                    title: "Export a new baseline from the current run".into(),
                    risk: RiskLevel::Low,
                    command: vec![
                        "assay".into(),
                        "run".into(),
                        "--export-baseline".into(),
                        ".assay/baseline.json".into(),
                    ],
                });
            }

            _ => {}
        }
    }

    // Deterministic order for agents
    actions.sort_by(|a, b| a.id.cmp(&b.id));
    patches.sort_by(|a, b| a.id.cmp(&b.id));

    (actions, patches)
}

fn policy_pointers(shape: PolicyShape) -> (&'static str, &'static str) {
    match shape {
        PolicyShape::TopLevel => ("/allow", "/deny"),
        PolicyShape::ToolsMap => ("/tools/allow", "/tools/deny"),
    }
}

fn detect_policy_shape(doc: &serde_yaml::Value) -> PolicyShape {
    // tools: { allow: [...], deny: [...] }
    if let Some(m) = doc.as_mapping() {
        if m.get(&serde_yaml::Value::String("tools".into())).is_some() {
            return PolicyShape::ToolsMap;
        }
    }
    PolicyShape::TopLevel
}

fn read_yaml(path: &Path) -> Option<serde_yaml::Value> {
    let s = std::fs::read_to_string(path).ok()?;
    serde_yaml::from_str::<serde_yaml::Value>(&s).ok()
}

fn best_candidate(ctx: &serde_json::Value) -> Option<String> {
    // Prefer candidates[0] if present; else none.
    ctx.get("candidates")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// --- JSON Pointer helpers for YAML doc inspection (only for indexing remove ops) ---

fn get_seq_strings(doc: &serde_yaml::Value, ptr: &str) -> Option<Vec<String>> {
    let node = yaml_ptr(doc, ptr)?;
    let seq = node.as_sequence()?;
    let mut out = Vec::new();
    for it in seq {
        if let Some(s) = it.as_str() {
            out.push(s.to_string());
        }
    }
    Some(out)
}

fn find_in_seq(doc: &serde_yaml::Value, ptr: &str, target: &str) -> Option<usize> {
    let node = yaml_ptr(doc, ptr)?;
    let seq = node.as_sequence()?;
    for (i, it) in seq.iter().enumerate() {
        if it.as_str() == Some(target) {
            return Some(i);
        }
    }
    None
}

fn yaml_ptr<'a>(root: &'a serde_yaml::Value, ptr: &str) -> Option<&'a serde_yaml::Value> {
    if ptr == "" || ptr == "/" {
        return Some(root);
    }
    let mut cur = root;
    for raw in ptr.trim_start_matches('/').split('/') {
        let key = unescape_pointer(raw);
        match cur {
            serde_yaml::Value::Mapping(m) => {
                cur = m.get(&serde_yaml::Value::String(key))?;
            }
            serde_yaml::Value::Sequence(seq) => {
                let idx: usize = key.parse().ok()?;
                cur = seq.get(idx)?;
            }
            _ => return None,
        }
    }
    Some(cur)
}

fn escape_pointer(s: &str) -> String {
    // JSON Pointer escaping
    s.replace('~', "~0").replace('/', "~1")
}
fn unescape_pointer(s: &str) -> String {
    s.replace("~1", "/").replace("~0", "~")
}
