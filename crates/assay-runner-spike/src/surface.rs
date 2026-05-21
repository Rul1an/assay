use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use thiserror::Error;

pub const CAPABILITY_SURFACE_SCHEMA: &str = "assay.runner.capability_surface.v0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySurface {
    pub schema: String,
    pub run_id: String,
    /// Set of filesystem paths the agent touched.
    ///
    /// v0 stores full paths from observed file events. Projection onto
    /// directory prefixes for capability-diff is a later transformation.
    pub filesystem_paths: BTreeSet<String>,
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
    #[error("capability surface run_id mismatch: expected {expected}, found {actual}")]
    RunIdMismatch { expected: String, actual: String },
}

impl CapabilitySurface {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            schema: CAPABILITY_SURFACE_SCHEMA.to_string(),
            run_id: run_id.into(),
            filesystem_paths: BTreeSet::new(),
            network_endpoints: BTreeSet::new(),
            process_execs: BTreeSet::new(),
            mcp_tools: BTreeSet::new(),
            policy_decisions: BTreeSet::new(),
        }
    }

    pub fn add_filesystem_path(&mut self, path: impl Into<String>) {
        self.filesystem_paths.insert(path.into());
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

    pub fn merge_from(&mut self, other: &Self) -> Result<(), CapabilitySurfaceError> {
        self.validate()?;
        other.validate()?;
        if self.run_id != other.run_id {
            return Err(CapabilitySurfaceError::RunIdMismatch {
                expected: self.run_id.clone(),
                actual: other.run_id.clone(),
            });
        }

        self.filesystem_paths
            .extend(other.filesystem_paths.iter().cloned());
        self.network_endpoints
            .extend(other.network_endpoints.iter().cloned());
        self.process_execs
            .extend(other.process_execs.iter().cloned());
        self.mcp_tools.extend(other.mcp_tools.iter().cloned());
        self.policy_decisions
            .extend(other.policy_decisions.iter().cloned());
        Ok(())
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
        surface.add_filesystem_path("/tmp/z");
        surface.add_filesystem_path("/tmp/a");
        surface.add_network_endpoint("b.example:443");
        surface.add_network_endpoint("a.example:443");
        surface.add_process_exec("/usr/bin/z");
        surface.add_process_exec("/usr/bin/a");
        surface.add_mcp_tool("write_file");
        surface.add_mcp_tool("read_file");
        surface.add_policy_decision("deny:write_file");
        surface.add_policy_decision("allow:read_file");

        let value = serde_json::to_value(&surface).expect("surface serializes");

        assert_eq!(
            value,
            json!({
                "schema": CAPABILITY_SURFACE_SCHEMA,
                "run_id": "run_001",
                "filesystem_paths": ["/tmp/a", "/tmp/z"],
                "network_endpoints": ["a.example:443", "b.example:443"],
                "process_execs": ["/usr/bin/a", "/usr/bin/z"],
                "mcp_tools": ["read_file", "write_file"],
                "policy_decisions": ["allow:read_file", "deny:write_file"]
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

    #[test]
    fn merge_from_preserves_deterministic_sets() {
        let mut first = CapabilitySurface::new("run_001");
        first.add_filesystem_path("/tmp/b");
        let mut second = CapabilitySurface::new("run_001");
        second.add_filesystem_path("/tmp/a");
        second.add_process_exec("/usr/bin/true");

        first.merge_from(&second).unwrap();

        assert_eq!(
            first.filesystem_paths.into_iter().collect::<Vec<_>>(),
            vec!["/tmp/a", "/tmp/b"]
        );
        assert_eq!(
            first.process_execs.into_iter().collect::<Vec<_>>(),
            vec!["/usr/bin/true"]
        );
    }

    #[test]
    fn merge_from_rejects_run_id_mismatch() {
        let mut first = CapabilitySurface::new("run_001");
        let second = CapabilitySurface::new("run_002");

        assert_eq!(
            first.merge_from(&second),
            Err(CapabilitySurfaceError::RunIdMismatch {
                expected: "run_001".to_string(),
                actual: "run_002".to_string(),
            })
        );
    }
}
