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
