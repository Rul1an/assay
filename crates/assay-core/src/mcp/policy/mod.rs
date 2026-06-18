mod contracts;
mod deserialize;
mod engine;
mod engine_next;
mod legacy;
mod matcher;
mod response;
mod schema;
mod types;

use super::identity::ToolIdentity;
use super::jcs;
use super::jsonrpc::JsonRpcRequest;
use crate::fingerprint::sha256_hex;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub use contracts::PolicyDecisionContract;
pub(in crate::mcp::policy) use matcher::matches_tool_pattern;
pub use response::make_deny_response;
pub use types::*;

impl McpPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_file(path: &std::path::Path) -> anyhow::Result<Self> {
        legacy::from_file(path)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        legacy::validate(self)
    }

    pub fn is_v1_format(&self) -> bool {
        legacy::is_v1_format(self)
    }

    /// Normalize legacy root-level allow/deny into tools.allow/deny.
    pub fn normalize_legacy_shapes(&mut self) {
        legacy::normalize_legacy_shapes(self);
    }

    /// Migrate V1 regex constraints to V2 JSON Schemas.
    /// Warning: This clears the `constraints` field.
    pub fn migrate_constraints_to_schemas(&mut self) {
        schema::migrate_constraints_to_schemas(self);
    }

    fn compiled_schemas(&self) -> &HashMap<String, Arc<jsonschema::Validator>> {
        self.compiled
            .get_or_init(|| schema::compile_all_schemas(self))
    }

    pub fn compile_all_schemas(&self) -> HashMap<String, Arc<jsonschema::Validator>> {
        schema::compile_all_schemas(self)
    }

    pub fn policy_digest(&self) -> Option<String> {
        let canonical = jcs::to_string(self).ok()?;
        Some(format!("sha256:{}", sha256_hex(&canonical)))
    }

    /// EXPERIMENTAL (unstable, may change): the declared-CONSTRAINT digest for the tool-decision
    /// truth-layer. Unlike `policy_digest` (the whole policy, Vec-structural), this projects to the
    /// declared-constraint surface only — `version`, the `tools` allow/deny + class/approval/scope lists,
    /// per-tool `schemas`, and `enforcement` — excluding operational knobs (`runtime_monitor`,
    /// `kill_switch`, `limits`, `discovery`, `signatures`, `tool_pins`, taxonomy), and SEMANTICALLY
    /// NORMALIZES the set-like fields (sorts them by canonical bytes) so a reordered-but-equal policy
    /// yields the same digest while a real membership/constraint change still moves it. Legacy v1 shapes
    /// are normalized first. Not a stability guarantee: names/shape may change until promoted out of
    /// experimental.
    pub fn declared_constraint_digest_experimental(&self) -> Option<String> {
        let mut p = self.clone();
        p.normalize_legacy_shapes();
        p.migrate_constraints_to_schemas();
        let full = serde_json::to_value(&p).ok()?;
        let proj = project_and_normalize_declared(&full);
        let canonical = jcs::to_string(&proj).ok()?;
        Some(format!("sha256:{}", sha256_hex(&canonical)))
    }

    /// Single evaluation entry point for CLI and Server
    pub fn evaluate(
        &self,
        tool_name: &str,
        args: &Value,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
    ) -> PolicyDecision {
        self.evaluate_with_metadata(tool_name, args, state, runtime_identity)
            .decision
    }

    pub fn evaluate_with_metadata(
        &self,
        tool_name: &str,
        args: &Value,
        state: &mut PolicyState,
        runtime_identity: Option<&ToolIdentity>,
    ) -> PolicyEvaluation {
        engine::evaluate_with_metadata(self, tool_name, args, state, runtime_identity)
    }

    // Proxy-specific check method (Legacy compatibility wrapper)
    pub fn check(&self, request: &JsonRpcRequest, state: &mut PolicyState) -> PolicyDecision {
        engine::check(self, request, state)
    }
}

// ── Declared-constraint projection + semantic normalization (EXPERIMENTAL) ───────────────────────
// Project to the declared-constraint surface, then sort the set-like fields by canonical bytes so a
// reordered-but-semantically-equal policy does not move the digest. Mirrors the tool-decision
// truth-layer reference-spec; unstable until promoted out of experimental.

fn sort_by_canon(arr: &mut [Value]) {
    arr.sort_by(|a, b| {
        jcs::to_string(a)
            .unwrap_or_default()
            .cmp(&jcs::to_string(b).unwrap_or_default())
    });
}

fn normalize_schema(sch: &Value) -> Value {
    let mut out = match sch.as_object() {
        Some(o) => o.clone(),
        None => return sch.clone(),
    };
    if let Some(req) = out.get("required").and_then(|r| r.as_array()) {
        let mut r = req.clone();
        sort_by_canon(&mut r);
        out.insert("required".to_string(), Value::Array(r));
    }
    if let Some(props) = out.get("properties").and_then(|p| p.as_object()) {
        let mut p = props.clone();
        for (field, spec) in props {
            if let Some(en) = spec.get("enum").and_then(|e| e.as_array()) {
                let mut e = en.clone();
                sort_by_canon(&mut e);
                let mut so = spec.as_object().cloned().unwrap_or_default();
                so.insert("enum".to_string(), Value::Array(e));
                p.insert(field.clone(), Value::Object(so));
            }
        }
        out.insert("properties".to_string(), Value::Object(p));
    }
    Value::Object(out)
}

fn project_and_normalize_declared(full: &Value) -> Value {
    let mut proj = serde_json::Map::new();
    if let Some(o) = full.as_object() {
        for key in ["version", "enforcement"] {
            if let Some(v) = o.get(key) {
                proj.insert(key.to_string(), v.clone());
            }
        }
        if let Some(tools) = o.get("tools").and_then(|t| t.as_object()) {
            let mut t = tools.clone();
            for k in [
                "allow",
                "deny",
                "allow_classes",
                "deny_classes",
                "approval_required",
                "approval_required_classes",
                "restrict_scope",
                "restrict_scope_classes",
            ] {
                if let Some(arr) = t.get(k).and_then(|a| a.as_array()) {
                    let mut a = arr.clone();
                    sort_by_canon(&mut a);
                    t.insert(k.to_string(), Value::Array(a));
                }
            }
            proj.insert("tools".to_string(), Value::Object(t));
        }
        if let Some(schemas) = o.get("schemas").and_then(|s| s.as_object()) {
            let mut s = serde_json::Map::new();
            for (name, sch) in schemas {
                s.insert(name.clone(), normalize_schema(sch));
            }
            proj.insert("schemas".to_string(), Value::Object(s));
        }
    }
    Value::Object(proj)
}

#[cfg(test)]
mod declared_constraint_digest_experimental_tests {
    use super::*;
    use serde_json::json;

    fn policy(allow: Value, extra: Value) -> McpPolicy {
        let mut v = json!({
            "version": "1",
            "tools": {"allow": allow, "deny": ["delete_all"]},
            "schemas": {"deploy": {"required": ["env"],
                "properties": {"env": {"type": "string", "enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        });
        if let (Some(o), Some(e)) = (v.as_object_mut(), extra.as_object()) {
            for (k, val) in e {
                o.insert(k.clone(), val.clone());
            }
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn reorder_allow_is_semantically_stable() {
        let a = policy(json!(["read_file", "list_dir", "deploy"]), json!({}))
            .declared_constraint_digest_experimental();
        let b = policy(json!(["deploy", "list_dir", "read_file"]), json!({}))
            .declared_constraint_digest_experimental();
        assert!(a.is_some());
        assert_eq!(a, b);
    }

    #[test]
    fn reorder_class_and_scope_lists_are_semantically_stable() {
        let a = policy(
            json!(["read_file"]),
            json!({"tools": {
                "allow": ["read_file"],
                "deny": ["delete_all"],
                "allow_classes": ["fs", "read"],
                "approval_required_classes": ["release", "prod"],
                "restrict_scope_classes": ["workspace", "repo"]
            }}),
        )
        .declared_constraint_digest_experimental();
        let b = policy(
            json!(["read_file"]),
            json!({"tools": {
                "allow": ["read_file"],
                "deny": ["delete_all"],
                "allow_classes": ["read", "fs"],
                "approval_required_classes": ["prod", "release"],
                "restrict_scope_classes": ["repo", "workspace"]
            }}),
        )
        .declared_constraint_digest_experimental();
        assert_eq!(a, b);
    }

    #[test]
    fn membership_change_moves_digest() {
        let a = policy(json!(["read_file", "list_dir", "deploy"]), json!({}))
            .declared_constraint_digest_experimental();
        let b = policy(json!(["read_file"]), json!({})).declared_constraint_digest_experimental();
        assert_ne!(a, b);
    }

    #[test]
    fn operational_change_is_stable() {
        let a = policy(
            json!(["read_file"]),
            json!({"runtime_monitor": {"enabled": true}, "limits": {"max_tool_calls_total": 100}}),
        )
        .declared_constraint_digest_experimental();
        let b = policy(
            json!(["read_file"]),
            json!({"runtime_monitor": {"enabled": false}, "limits": {"max_tool_calls_total": 1}}),
        )
        .declared_constraint_digest_experimental();
        assert_eq!(a, b);
    }
}
