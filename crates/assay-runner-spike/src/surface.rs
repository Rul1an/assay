use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const CAPABILITY_SURFACE_SCHEMA: &str = "assay.runner.capability_surface.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySurface {
    pub schema: String,
    pub run_id: String,
    pub filesystem_prefixes: BTreeSet<String>,
    pub network_endpoints: BTreeSet<String>,
    pub process_execs: BTreeSet<String>,
    pub mcp_tools: BTreeSet<String>,
    pub policy_decisions: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilitySurfaceError {
    #[error("capability surface schema must be {CAPABILITY_SURFACE_SCHEMA}")]
    InvalidSchema,
    #[error("run_id must not be empty")]
    EmptyRunId,
}

impl CapabilitySurface {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            schema: CAPABILITY_SURFACE_SCHEMA.to_string(),
            run_id: run_id.into(),
            filesystem_prefixes: BTreeSet::new(),
            network_endpoints: BTreeSet::new(),
            process_execs: BTreeSet::new(),
            mcp_tools: BTreeSet::new(),
            policy_decisions: BTreeSet::new(),
        }
    }

    pub fn add_filesystem_prefix(&mut self, prefix: impl Into<String>) {
        self.filesystem_prefixes.insert(prefix.into());
    }

    pub fn add_network_endpoint(&mut self, endpoint: impl Into<String>) {
        self.network_endpoints.insert(endpoint.into());
    }

    pub fn add_process_exec(&mut self, exec: impl Into<String>) {
        self.process_execs.insert(exec.into());
    }

    pub fn add_mcp_tool(&mut self, tool: impl Into<String>) {
        self.mcp_tools.insert(tool.into());
    }

    pub fn add_policy_decision(&mut self, decision: impl Into<String>) {
        self.policy_decisions.insert(decision.into());
    }

    pub fn validate(&self) -> Result<(), CapabilitySurfaceError> {
        if self.schema != CAPABILITY_SURFACE_SCHEMA {
            return Err(CapabilitySurfaceError::InvalidSchema);
        }
        if self.run_id.is_empty() {
            return Err(CapabilitySurfaceError::EmptyRunId);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capability_surface_serializes_sets_deterministically() {
        let mut surface = CapabilitySurface::new("run_001");
        surface.add_filesystem_prefix("/tmp/z");
        surface.add_filesystem_prefix("/tmp/a");
        surface.add_network_endpoint("b.example:443");
        surface.add_network_endpoint("a.example:443");
        surface.add_process_exec("/usr/bin/z");
        surface.add_process_exec("/usr/bin/a");
        surface.add_mcp_tool("filesystem.write_file");
        surface.add_mcp_tool("filesystem.read_file");
        surface.add_policy_decision("deny:filesystem.write_file");
        surface.add_policy_decision("allow:filesystem.read_file");

        let value = serde_json::to_value(&surface).expect("surface serializes");

        assert_eq!(
            value,
            json!({
                "schema": CAPABILITY_SURFACE_SCHEMA,
                "run_id": "run_001",
                "filesystem_prefixes": ["/tmp/a", "/tmp/z"],
                "network_endpoints": ["a.example:443", "b.example:443"],
                "process_execs": ["/usr/bin/a", "/usr/bin/z"],
                "mcp_tools": ["filesystem.read_file", "filesystem.write_file"],
                "policy_decisions": ["allow:filesystem.read_file", "deny:filesystem.write_file"]
            })
        );
    }

    #[test]
    fn validate_rejects_unexpected_schema() {
        let mut surface = CapabilitySurface::new("run_001");
        surface.schema = "assay.runner.capability_surface.v_future".to_string();

        assert_eq!(
            surface.validate(),
            Err(CapabilitySurfaceError::InvalidSchema)
        );
    }
}
