// crates/assay-core/src/agentic/mod.rs
// A reusable "Agentic Contract" builder that turns Diagnostics into:
// - suggested_actions (commands to run)
// - suggested_patches (JSON Patch ops, machine-applyable)
//
// This is intentionally conservative + deterministic.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
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
    // Deduplication maps
    let mut actions_map: BTreeMap<String, SuggestedAction> = BTreeMap::new();
    let mut patches_map: BTreeMap<String, SuggestedPatch> = BTreeMap::new();

    // Default policy path lookup
    let default_policy = ctx
        .policy_path
        .clone()
        .unwrap_or_else(|| PathBuf::from("policy.yaml"));

    // We only read the policy once relative to CWD if possible,
    // but individual diagnostics might point to other files.
    // For shape detection, we rely on the default policy file.
    let policy_doc = read_yaml(&default_policy);
    let policy_shape = policy_doc
        .as_ref()
        .map(|doc| detect_policy_shape(doc))
        .unwrap_or(PolicyShape::TopLevel);

    for d in diags {
        // Resolve policy path for this specific diagnostic
        // Priority: 1. diag.context.policy_file, 2. ctx.policy_path, 3. "policy.yaml"
        let policy_path_str = d
            .context
            .get("policy_file")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| default_policy.display().to_string());

        match d.code.as_str() {
            // ----------------------------
            // YAML parse errors -> actions
            // ----------------------------
            "E_CFG_PARSE" | "E_POLICY_PARSE" => {
                let id = "regen_config".to_string();
                actions_map.insert(id.clone(), SuggestedAction {
                    id,
                    title: "Regenerate a clean config (does not overwrite existing files)".into(),
                    risk: RiskLevel::Low,
                    command: vec!["assay".into(), "init".into()],
                });
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

                    patches_map.insert(id.clone(), SuggestedPatch {
                        id,
                        title: format!("Rename field '{}' to '{}'", unknown, suggested),
                        risk: RiskLevel::Low,
                        file: file.to_string(),
                        ops: vec![JsonPatchOp::Move { from, path: to }],
                    });
                }
            }

            // --------------------------------------------------
            // Unknown tool -> Action only (no patch, safer)
            // --------------------------------------------------
            "UNKNOWN_TOOL" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let id = format!("fix_unknown_tool:{}", tool);
                    actions_map.insert(id.clone(), SuggestedAction {
                        id,
                        title: format!("Verify if tool '{}' exists and is named correctly in policy", tool),
                        risk: RiskLevel::Low,
                        command: vec![
                            "assay".into(),
                            "doctor".into(),
                            "--format".into(),
                            "json".into()
                        ],
                    });
                }
            }

            // --------------------------------------------------
            // Tool not allowed -> add to allowlist (Patch)
            // --------------------------------------------------
            "MCP_TOOL_NOT_ALLOWED" | "E_TOOL_NOT_ALLOWED" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let (allow_ptr, _) = policy_pointers(policy_shape);

                    // If allowlist is "*" then tool-not-allowed is likely not the issue.
                    let allow_is_wildcard = policy_doc
                        .as_ref()
                        .and_then(|doc| get_seq_strings(doc, allow_ptr))
                        .map(|xs| xs.iter().any(|s| s == "*"))
                        .unwrap_or(false);

                    if !allow_is_wildcard {
                        let id = format!("allow_tool:{}", tool);
                        patches_map.insert(id.clone(), SuggestedPatch {
                            id,
                            title: format!("Allow tool '{}'", tool),
                            risk: RiskLevel::High,
                            file: policy_path_str.clone(),
                            ops: vec![JsonPatchOp::Add {
                                path: format!("{}/-", allow_ptr),
                                value: JsonValue::String(tool.to_string()),
                            }],
                        });
                    }
                }
            }

            // --------------------------------------------------
            // Tool denied -> remove from denylist (High risk)
            // --------------------------------------------------
            "E_EXEC_DENIED" | "MCP_TOOL_DENIED" | "E_TOOL_DENIED" => {
                if let Some(tool) = d.context.get("tool").and_then(|v: &JsonValue| v.as_str()) {
                    let (_, deny_ptr) = policy_pointers(policy_shape);

                    if let Some(doc) = policy_doc.as_ref() {
                        if let Some(idx) = find_in_seq(doc, deny_ptr, tool) {
                            let id = format!("remove_deny:{}", tool);
                            patches_map.insert(id.clone(), SuggestedPatch {
                                id,
                                title: format!("Remove '{}' from denylist", tool),
                                risk: RiskLevel::High,
                                file: policy_path_str.clone(),
                                ops: vec![JsonPatchOp::Remove {
                                    path: format!("{}/{}", deny_ptr, idx),
                                }],
                            });
                        } else {
                            let id = format!("manual_remove_deny:{}", tool);
                            actions_map.insert(id.clone(), SuggestedAction {
                                id,
                                title: format!(
                                    "Manually remove '{}' from denylist in {}",
                                    tool,
                                    policy_path_str
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
                patches_map.insert(id.clone(), SuggestedPatch {
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
                });
            }

            // --------------------------------------------------
            // Tool poisoning -> Action only (Conservative for v1.5)
            // Avoids complex Replace/Add branching without EnsureObject
            // --------------------------------------------------
            "E_TOOL_POISONING_PATTERN" | "E_TOOL_DESC_SUSPICIOUS" | "E_SIGNATURES_DISABLED" => {
                let id = "enable_tool_poisoning_checks".to_string();
                actions_map.insert(id.clone(), SuggestedAction {
                    id,
                    title: format!("Enable tool poisoning heuristics (check_descriptions) in {}", policy_path_str),
                    risk: RiskLevel::Low,
                    command: vec![
                        "assay".into(),
                        "fix".into(),
                        "--config".into(),
                        policy_path_str.clone()
                    ],
                });
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
                                patches_map.insert(id.clone(), SuggestedPatch {
                                    id,
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
                            let id = "fix_baseline_path".to_string();
                            patches_map.insert(id.clone(), SuggestedPatch {
                                id,
                                title: "Set baseline path to .assay/baseline.json".into(),
                                risk: RiskLevel::Low,
                                file: file.to_string(),
                                ops: vec![JsonPatchOp::Replace {
                                    path: "/baseline".into(),
                                    value: JsonValue::String(".assay/baseline.json".into()),
                                }],
                            });

                            let action_id = "create_baseline_dir".to_string();
                            actions_map.insert(action_id.clone(), SuggestedAction {
                                id: action_id,
                                title: "Create baseline directory".into(),
                                risk: RiskLevel::Low,
                                command: vec!["mkdir".into(), "-p".into(), ".assay".into()],
                            });
                        }
                    }
                }
            }

            // --------------------------------------------------
            // Trace drift -> action with context-aware filename
            // --------------------------------------------------
            "E_TRACE_SCHEMA_DRIFT" | "E_TRACE_SCHEMA_INVALID" | "E_TRACE_LEGACY_FUNCTION_CALL" => {
                 let raw_trace_file = d.context
                    .get("trace_file")
                    .and_then(|v| v.as_str())
                    .unwrap_or("<raw.jsonl>");

                let id = "normalize_trace".to_string();
                actions_map.insert(id.clone(), SuggestedAction {
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
                });
            }

            // --------------------------------------------------
            // Baseline mismatch
            // --------------------------------------------------
            "E_BASE_MISMATCH" | "E_BASELINE_SUITE_MISMATCH" => {
                let id = "export_baseline".to_string();
                actions_map.insert(id.clone(), SuggestedAction {
                    id,
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

    // Convert BTreeMaps to Vecs (already sorted by id key)
    (
        actions_map.into_values().collect(),
        patches_map.into_values().collect(),
    )
}

fn policy_pointers(shape: PolicyShape) -> (&'static str, &'static str) {
    match shape {
        PolicyShape::TopLevel => ("/allow", "/deny"),
        PolicyShape::ToolsMap => ("/tools/allow", "/tools/deny"),
    }
}

fn detect_policy_shape(doc: &serde_yaml::Value) -> PolicyShape {
    // Check if `tools` key exists and is a mapping
    let tools = doc
        .as_mapping()
        .and_then(|m| m.get(&serde_yaml::Value::String("tools".into())))
        .and_then(|v| v.as_mapping());

    if let Some(tm) = tools {
        // Robust check: it's only the "ToolsMap" shape if allow/deny are SEQUENCES inside tools
        let has_allow = tm
            .get(&serde_yaml::Value::String("allow".into()))
            .and_then(|v| v.as_sequence())
            .is_some();
        let has_deny = tm
            .get(&serde_yaml::Value::String("deny".into()))
            .and_then(|v| v.as_sequence())
            .is_some();

        if has_allow || has_deny {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_deduplication() {
        let diags = vec![
            Diagnostic::new("E_CFG_PARSE", "Error 1"),
            Diagnostic::new("E_CFG_PARSE", "Error 2"),
        ];
        let ctx = AgenticCtx { policy_path: None };
        let (actions, patches) = build_suggestions(&diags, &ctx);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, "regen_config");
        assert!(patches.is_empty());
    }

    #[test]
    fn test_unknown_tool_action_only() {
        let mut d = Diagnostic::new("UNKNOWN_TOOL", "Unknown tool");
        d.context = json!({ "tool": "weird-tool" });

        let diags = vec![d];
        let ctx = AgenticCtx { policy_path: None };
        let (actions, patches) = build_suggestions(&diags, &ctx);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].id, "fix_unknown_tool:weird-tool");
        assert!(patches.is_empty(), "UNKNOWN_TOOL should not generate patches");
    }

    #[test]
    fn test_rename_field_patch() {
        let mut d = Diagnostic::new("E_CFG_SCHEMA_UNKNOWN_FIELD", "Unknown field");
        d.context = json!({
            "file": "assay.yaml",
            "json_pointer_parent": "/config",
            "unknown_field": "policcy",
            "suggested_field": "policy"
        });

        let diags = vec![d];
        let ctx = AgenticCtx { policy_path: None };
        let (_, patches) = build_suggestions(&diags, &ctx);

        assert_eq!(patches.len(), 1);
        let p = &patches[0];
        assert_eq!(p.id, "rename_field:policcy->policy");

        match &p.ops[0] {
            JsonPatchOp::Move { from, path } => {
                assert_eq!(from, "/config/policcy");
                assert_eq!(path, "/config/policy");
            }
            _ => panic!("Expected Move op"),
        }
    }

    #[test]
    fn test_detect_policy_shape() {
        // Top Level
        let doc1: serde_yaml::Value = serde_yaml::from_str("allow: []\ndeny: []").unwrap();
        match detect_policy_shape(&doc1) {
            PolicyShape::TopLevel => {},
            _ => panic!("Expected TopLevel"),
        }

        // Tools Map (Legacy/Standard)
        let doc2: serde_yaml::Value = serde_yaml::from_str(r#"
tools:
  allow: ["read_file"]
  deny: []
"#).unwrap();
        match detect_policy_shape(&doc2) {
            PolicyShape::ToolsMap => {},
            _ => panic!("Expected ToolsMap"),
        }

        // Tools as explicit map (Bug regression check)
        // If tools is just a map of definitions, it should NOT be detected as ToolsMap
        // unless it has allow/deny sequences.
        let doc3: serde_yaml::Value = serde_yaml::from_str(r#"
tools:
  my-tool:
    image: python:3.9
"#).unwrap();
        match detect_policy_shape(&doc3) {
            PolicyShape::TopLevel => {},
            _ => panic!("Expected TopLevel for tools definition map"),
        }
    }
}
