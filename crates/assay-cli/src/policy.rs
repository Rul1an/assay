//! Policy loading and merge logic for Assay Sandbox.
//! Implements ADR-001 semantics: Allow-Union, Deny-Wins.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Top-level sandbox policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Policy {
    #[serde(default = "default_api_version")]
    pub api_version: String,
    #[serde(default)]
    pub extends: Vec<String>,
    #[serde(default)]
    pub fs: FsPolicy,
    #[serde(default)]
    pub net: NetPolicy,
}

fn default_api_version() -> String {
    "assay/v1".to_string()
}

/// Filesystem access policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FsPolicy {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Network access policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetPolicy {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

impl Policy {
    /// Load policy from YAML file.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let policy: Policy = serde_yaml::from_str(&content)?;
        Ok(policy)
    }

    /// Merge another policy into this one (union semantics).
    /// After merge, use `effective_*` methods to apply deny-wins logic.
    pub fn merge(&mut self, other: Policy) {
        // Union allows
        self.fs.allow.extend(other.fs.allow);
        self.net.allow.extend(other.net.allow);
        // Union denies
        self.fs.deny.extend(other.fs.deny);
        self.net.deny.extend(other.net.deny);
        // Deduplicate
        self.fs.allow = dedupe(&self.fs.allow);
        self.fs.deny = dedupe(&self.fs.deny);
        self.net.allow = dedupe(&self.net.allow);
        self.net.deny = dedupe(&self.net.deny);
    }

    /// Get effective FS allows (deny-wins: remove any allow that has a matching deny).
    pub fn effective_fs_allow(&self) -> Vec<&str> {
        let deny_set: HashSet<_> = self.fs.deny.iter().map(|s| s.as_str()).collect();
        self.fs
            .allow
            .iter()
            .filter(|a| !deny_set.contains(a.as_str()))
            .map(|s| s.as_str())
            .collect()
    }

    /// Check if a path is denied (exact match for now).
    pub fn is_fs_denied(&self, path: &str) -> bool {
        self.fs.deny.iter().any(|d| path.starts_with(d))
    }

    /// Get rule counts for display.
    pub fn rule_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.fs.allow.len(),
            self.fs.deny.len(),
            self.net.allow.len(),
            self.net.deny.len(),
        )
    }
}

fn dedupe(v: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    v.iter()
        .filter(|s| seen.insert(s.as_str()))
        .cloned()
        .collect()
}

/// Load the built-in MCP Server Minimal pack.
pub fn mcp_server_minimal() -> Policy {
    Policy {
        api_version: "assay/v1".to_string(),
        extends: vec![],
        fs: FsPolicy {
            allow: vec![],
            deny: vec![
                "/etc/shadow".to_string(),
                "/etc/passwd".to_string(),
                "~/.ssh".to_string(),
                "~/.aws".to_string(),
                "~/.config/gh".to_string(),
                "~/.npmrc".to_string(),
                "~/.netrc".to_string(),
            ],
        },
        net: NetPolicy {
            allow: vec!["127.0.0.1".to_string(), "localhost".to_string()],
            deny: vec!["0.0.0.0/0".to_string()], // Deny all outbound by default
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_union() {
        let mut a = Policy {
            fs: FsPolicy {
                allow: vec!["/tmp".to_string()],
                deny: vec![],
            },
            ..Default::default()
        };
        let b = Policy {
            fs: FsPolicy {
                allow: vec!["/var".to_string()],
                deny: vec!["/etc".to_string()],
            },
            ..Default::default()
        };
        a.merge(b);
        assert_eq!(a.fs.allow, vec!["/tmp", "/var"]);
        assert_eq!(a.fs.deny, vec!["/etc"]);
    }

    #[test]
    fn test_deny_wins() {
        let policy = Policy {
            fs: FsPolicy {
                allow: vec!["/etc".to_string(), "/tmp".to_string()],
                deny: vec!["/etc".to_string()],
            },
            ..Default::default()
        };
        let effective = policy.effective_fs_allow();
        assert_eq!(effective, vec!["/tmp"]);
    }

    #[test]
    fn test_mcp_minimal_pack() {
        let pack = mcp_server_minimal();
        assert!(pack.fs.deny.contains(&"/etc/shadow".to_string()));
        assert!(pack.net.deny.contains(&"0.0.0.0/0".to_string()));
    }
}
