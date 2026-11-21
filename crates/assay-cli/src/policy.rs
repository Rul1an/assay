//! Policy loading and merge logic for Assay Sandbox.
//! Implements ADR-001 semantics: Allow-Union, Deny-Wins.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Runtime sandbox policy, also serialized by the profile writer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct FsPolicy {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

/// Network access policy.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct NetPolicy {
    #[serde(default)]
    pub allow: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

assay_common::closed! {
    root struct SandboxDocument {
        #[default = "default_api_version"]
        api_version: String,
        #[default]
        extends: Vec<String>,
        #[default]
        fs: PathRules,
        #[default]
        net: PathRules,
    }
}

assay_common::closed! {
    #[derive(Default)]
    struct PathRules {
        #[default]
        allow: Vec<String>,
        #[default]
        deny: Vec<String>,
    }
}

/// The root keys `assay sandbox --profile` wrote into its suggestion up to 6.23.0, which the
/// loader never read.
const EARLIER_SUGGESTION_KEYS: &[&str] = &["env", "processes", "meta"];

fn key_name(key: &serde_yaml::Value) -> String {
    match key.as_str() {
        Some(name) => name.to_owned(),
        None => serde_yaml::to_string(key)
            .map(|text| text.trim_end().to_owned())
            .unwrap_or_else(|_| "<key>".to_owned()),
    }
}

/// The refusal for a file with keys the loader does not read. Each key is written with Debug
/// quoting, so a control character in a key reaches the terminal escaped.
fn unread_keys_refusal(unread: &[String]) -> anyhow::Error {
    let named: Vec<String> = unread.iter().map(|key| format!("{key:?}")).collect();
    let mut message = format!(
        "assay sandbox does not read {}; a sandbox policy has api_version, extends, fs (allow, \
         deny) and net (allow, deny)",
        named.join(", ")
    );
    if unread
        .iter()
        .any(|key| EARLIER_SUGGESTION_KEYS.contains(&key.as_str()))
    {
        message.push_str(
            ". assay sandbox --profile wrote env, processes and meta into earlier suggestions and \
             assay sandbox never read them; delete those blocks",
        );
    }
    anyhow::anyhow!(message)
}

impl Policy {
    /// Load policy from YAML file.
    ///
    /// A key the loader does not read is refused, all of them named at once, before anything else
    /// about the file is checked. An ignored key would mean running with less than the file says,
    /// which ADR-043 §5 refuses for a named policy as it refuses a non-empty `extends`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        use assay_common::doc::{Input, Refusal};

        let content = assay_core::model::read_policy_text(path)?;
        let SandboxDocument {
            api_version,
            extends,
            fs:
                PathRules {
                    allow: fs_allow,
                    deny: fs_deny,
                },
            net:
                PathRules {
                    allow: net_allow,
                    deny: net_deny,
                },
        } = Input::parse(&content)
            .and_then(|input| input.read_unknown_keys_first(key_name))
            .map_err(|error| match error {
                Refusal::UnknownKeys(mut keys) => {
                    keys.sort_unstable();
                    unread_keys_refusal(&keys)
                }
                Refusal::Syntax { message, .. } | Refusal::Shape { message, .. } => {
                    anyhow::anyhow!("{message}")
                }
                Refusal::NotAMapping { path, .. } if path.is_empty() => {
                    anyhow::anyhow!("sandbox policy root must be a mapping")
                }
                Refusal::NotAMapping { path, .. } => {
                    anyhow::anyhow!("expected a mapping at sandbox policy key {path:?}")
                }
                _ => anyhow::anyhow!("invalid sandbox policy"),
            })?;
        if !extends.is_empty() {
            anyhow::bail!(
                "sandbox policy loading does not support non-empty `extends`; \
                 remove the entries because pack resolution is unsupported"
            );
        }
        Ok(Self {
            api_version,
            extends,
            fs: FsPolicy {
                allow: fs_allow,
                deny: fs_deny,
            },
            net: NetPolicy {
                allow: net_allow,
                deny: net_deny,
            },
        })
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

    /// Every policy example on the pages that teach the sandbox policy loads. A block is taken as
    /// a sandbox policy when its root names `api_version`, `fs` or `net`; other YAML on those
    /// pages (CI snippets, other formats) is not.
    #[test]
    fn documented_sandbox_policy_examples_load() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut checked = 0;
        for page in [
            "docs/reference/sandbox-policies.md",
            "docs/reference/cli/sandbox.md",
            "docs/guides/sandbox-security.md",
        ] {
            let text = std::fs::read_to_string(root.join(page)).expect(page);
            for block in text.split("```yaml\n").skip(1) {
                let block = block.split("```").next().expect("fenced block");
                let Ok(document) = serde_yaml::from_str::<serde_yaml::Value>(block) else {
                    continue;
                };
                let is_policy = document.as_mapping().is_some_and(|m| {
                    ["api_version", "fs", "net"]
                        .iter()
                        .any(|key| m.contains_key(*key))
                });
                if !is_policy {
                    continue;
                }
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("policy.yaml");
                std::fs::write(&path, block).expect("write example");
                Policy::load(&path).unwrap_or_else(|e| panic!("{page}:\n{block}\n{e:#}"));
                checked += 1;
            }
        }
        assert!(
            checked >= 10,
            "found only {checked} sandbox policy examples"
        );
    }

    #[test]
    fn profile_writer_and_reader_have_the_same_fields() {
        use assay_core::unread_keys::keys_of;
        let written = serde_json::to_value(Policy::default()).unwrap();
        for (object, expected) in [
            (&written, keys_of::<SandboxDocument>().unwrap()),
            (&written["fs"], keys_of::<PathRules>().unwrap()),
            (&written["net"], keys_of::<PathRules>().unwrap()),
        ] {
            let actual: std::collections::BTreeSet<_> = object
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            assert_eq!(actual, expected.iter().copied().collect());
        }
    }

    #[test]
    fn test_mcp_minimal_pack() {
        let pack = mcp_server_minimal();
        assert!(pack.fs.deny.contains(&"/etc/shadow".to_string()));
        assert!(pack.net.deny.contains(&"0.0.0.0/0".to_string()));
    }
}
