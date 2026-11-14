//! Trust policy for tool signature verification.
//!
//! Defines which signing keys are trusted for tool verification.

use anyhow::{Context, Result};
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use super::signing::compute_key_id_from_verifying_key;

/// Trust policy for tool signature verification.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustPolicy {
    /// If true, unsigned tools are rejected.
    #[serde(default)]
    pub require_signed: bool,

    /// Trusted key IDs (sha256:...) with no key behind them.
    ///
    /// A key id alone verifies no signature: [`TrustPolicy::load_keys`] loads keys only from
    /// `trusted_keys`, so a tool signed by a key listed here is reported trusted and then fails.
    /// A trust policy file that sets this key is refused by [`TrustPolicy::from_yaml`]; the field
    /// stays for policies built in code.
    #[serde(default)]
    pub trusted_key_ids: Vec<String>,

    /// Detailed trusted keys with metadata.
    #[serde(default)]
    pub trusted_keys: Vec<TrustedKey>,
}

/// A trusted key with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedKey {
    /// SHA-256 of SPKI bytes: `sha256:<hex>`
    pub key_id: String,

    /// Path to public key file (SPKI PEM).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key_path: Option<PathBuf>,

    /// Friendly name for display.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl TrustPolicy {
    /// Load trust policy from YAML file.
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("failed to read trust policy: {}", path.display()))?;
        Self::from_yaml(&content)
    }

    /// Parse a trust policy file as `assay mcp tool verify` reads it.
    ///
    /// The file is read into the private `view` module's types, which hold only the keys the
    /// verifier acts on, and every other key refuses the policy, named by its path. serde's
    /// defaults would otherwise drop such a key without a word: `require_signd: true` left
    /// `require_signed` false, and an unsigned tool passed. `trusted_key_ids` is refused with the
    /// entry that does verify.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let mut unread = Vec::new();
        let file: view::TrustPolicyFile =
            serde_ignored::deserialize(serde_yaml::Deserializer::from_str(yaml), |path| {
                unread.push(key_path(&path.to_string()))
            })
            .context("failed to parse trust policy YAML")?;
        if !unread.is_empty() {
            anyhow::bail!("{}", refusal(&unread));
        }
        Ok(Self {
            require_signed: file.require_signed,
            trusted_key_ids: Vec::new(),
            trusted_keys: file
                .trusted_keys
                .into_iter()
                .map(|key| TrustedKey {
                    key_id: key.key_id,
                    public_key_path: key.public_key_path,
                    name: key.name,
                })
                .collect(),
        })
    }

    /// Create an empty policy (allows everything unsigned).
    pub fn permissive() -> Self {
        Self::default()
    }

    /// Create a policy that requires signatures but trusts any valid signature.
    pub fn require_signed_any() -> Self {
        Self {
            require_signed: true,
            ..Default::default()
        }
    }

    /// Create a policy from a single public key.
    pub fn from_key(key: &VerifyingKey) -> Result<Self> {
        let key_id = compute_key_id_from_verifying_key(key)?;
        Ok(Self {
            require_signed: true,
            trusted_key_ids: vec![key_id],
            trusted_keys: vec![],
        })
    }

    /// Get all trusted key IDs (from both trusted_key_ids and trusted_keys).
    pub fn all_trusted_key_ids(&self) -> HashSet<&str> {
        let mut ids: HashSet<&str> = self.trusted_key_ids.iter().map(|s| s.as_str()).collect();
        for key in &self.trusted_keys {
            ids.insert(&key.key_id);
        }
        ids
    }

    /// Check if a key_id is trusted.
    ///
    /// # Security Note
    ///
    /// If no trusted keys are configured (`trusted_key_ids` and `trusted_keys` are both empty),
    /// this returns `true` for ANY key_id (permissive mode). This is intentional for development
    /// workflows but **must not be used in production**. Always configure explicit trusted keys
    /// for production deployments.
    ///
    /// To enforce strict verification, either:
    /// - Add at least one `trusted_key_ids` entry
    /// - Add at least one `trusted_keys` entry with `key_id`
    pub fn is_key_trusted(&self, key_id: &str) -> bool {
        // SECURITY: If no keys are configured, trust all (permissive mode for dev).
        // Production deployments MUST configure explicit trusted keys.
        if self.trusted_key_ids.is_empty() && self.trusted_keys.is_empty() {
            return true;
        }
        self.all_trusted_key_ids().contains(key_id)
    }

    /// Load all public keys from trusted_keys paths.
    pub fn load_keys(&self) -> Result<Vec<LoadedKey>> {
        let mut loaded = Vec::new();

        for trusted in &self.trusted_keys {
            if let Some(path) = &trusted.public_key_path {
                let key = load_public_key_pem(path)?;
                let actual_key_id = compute_key_id_from_verifying_key(&key)?;

                // Verify key_id matches
                if actual_key_id != trusted.key_id {
                    anyhow::bail!(
                        "key_id mismatch for {}: expected {}, got {}",
                        path.display(),
                        trusted.key_id,
                        actual_key_id
                    );
                }

                loaded.push(LoadedKey {
                    key_id: trusted.key_id.clone(),
                    key,
                    name: trusted.name.clone(),
                });
            }
        }

        Ok(loaded)
    }
}

/// The trust policy file as `assay mcp tool verify` reads it: every key it acts on, and nothing
/// else. Under `forbid(dead_code)` a field here that nothing reads is a compile error, because
/// `derive(Deserialize)` writes fields and does not read them; a key the verifier stops reading
/// must leave this module, and is then refused. A read is not an effect: what each field does is
/// the verifier's (`crates/assay-cli/src/cli/commands/tool/verify.rs`).
mod view {
    #![forbid(dead_code)]

    use serde::Deserialize;
    use std::path::PathBuf;

    #[derive(Deserialize)]
    pub(super) struct TrustPolicyFile {
        /// An unsigned tool fails when this is true and passes when it is false.
        #[serde(default)]
        pub(super) require_signed: bool,
        /// The only keys a signature is verified against.
        #[serde(default)]
        pub(super) trusted_keys: Vec<TrustedKeyEntry>,
    }

    #[derive(Deserialize)]
    pub(super) struct TrustedKeyEntry {
        /// Must equal the key id of the key at `public_key_path` and of the signature.
        pub(super) key_id: String,
        /// The key a signature is verified against; an entry without one verifies nothing.
        pub(super) public_key_path: Option<PathBuf>,
        /// Shown when this key verifies a tool.
        pub(super) name: Option<String>,
    }
}

/// `serde_ignored` writes a `?` segment for each `Option` it passes through; a key path has none.
fn key_path(path: &str) -> String {
    path.split('.')
        .filter(|segment| !segment.is_empty() && *segment != "?")
        .collect::<Vec<_>>()
        .join(".")
}

/// One line that names every refused key, escaped, in file order.
fn refusal(unread: &[String]) -> String {
    let keys: Vec<String> = unread
        .iter()
        .map(|path| {
            let why = if path == "trusted_key_ids" || path.starts_with("trusted_key_ids.") {
                "a key id alone verifies no signature; list the key under trusted_keys with its \
                 key_id and public_key_path"
            } else {
                "assay mcp tool verify does not act on this key"
            };
            format!("{path:?}: {why}")
        })
        .collect();
    format!(
        "assay mcp tool verify refuses this trust policy: {} key(s) it does not act on: {}. \
         Correct or remove them.",
        keys.len(),
        keys.join("; ")
    )
}

/// A loaded public key with metadata.
#[derive(Debug)]
pub struct LoadedKey {
    pub key_id: String,
    pub key: VerifyingKey,
    pub name: Option<String>,
}

/// Load a public key from SPKI PEM file.
pub fn load_public_key_pem(path: &Path) -> Result<VerifyingKey> {
    use ed25519_dalek::pkcs8::DecodePublicKey;

    let pem = fs::read_to_string(path)
        .with_context(|| format!("failed to read public key: {}", path.display()))?;

    VerifyingKey::from_public_key_pem(&pem)
        .with_context(|| format!("failed to parse public key PEM: {}", path.display()))
}

/// Load a private key from PKCS#8 PEM file.
pub fn load_private_key_pem(path: &Path) -> Result<ed25519_dalek::SigningKey> {
    use ed25519_dalek::pkcs8::DecodePrivateKey;

    let pem = fs::read_to_string(path)
        .with_context(|| format!("failed to read private key: {}", path.display()))?;

    ed25519_dalek::SigningKey::from_pkcs8_pem(&pem)
        .with_context(|| format!("failed to parse private key PEM: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_policy_yaml() {
        let yaml = r#"
require_signed: true
trusted_keys:
  - key_id: "sha256:789xyz"
    name: "CI Key"
    public_key_path: "./keys/ci-public.pem"
"#;

        let policy = TrustPolicy::from_yaml(yaml).unwrap();
        assert!(policy.require_signed);
        assert!(policy.trusted_key_ids.is_empty());
        assert_eq!(policy.trusted_keys.len(), 1);
        assert_eq!(policy.trusted_keys[0].name, Some("CI Key".to_string()));
        assert_eq!(
            policy.trusted_keys[0].public_key_path,
            Some(PathBuf::from("./keys/ci-public.pem"))
        );
    }

    /// Every key the verifier does not act on refuses the file, named by its path in file order;
    /// `trusted_key_ids` with the entry that does verify.
    #[test]
    fn test_a_key_the_verifier_does_not_act_on_refuses_the_policy() {
        let yaml = r#"
require_signd: true
trusted_key_ids: ["sha256:abc123", "sha256:def456"]
trusted_keys:
  - key_id: "sha256:789xyz"
    public_key_pth: "./keys/ci-public.pem"
"#;
        let error = TrustPolicy::from_yaml(yaml).unwrap_err().to_string();
        assert!(
            error.starts_with("assay mcp tool verify refuses this trust policy: 3 key(s)"),
            "{error}"
        );
        let named: Vec<usize> = [
            "\"require_signd\"",
            "\"trusted_key_ids\"",
            "\"trusted_keys.0.public_key_pth\"",
        ]
        .iter()
        .map(|key| {
            error
                .find(key)
                .unwrap_or_else(|| panic!("{key} not named: {error}"))
        })
        .collect();
        assert!(
            named.windows(2).all(|pair| pair[0] < pair[1]),
            "file order: {error}"
        );
        assert!(error.contains("list the key under trusted_keys"), "{error}");

        // Control: the same file without those keys loads.
        assert!(TrustPolicy::from_yaml("require_signed: true\ntrusted_keys: []\n").is_ok());
    }

    /// A refused key is named as the file spells it, `?` and all. No field of the view is an
    /// `Option` of a section, so a `?` in a path is the key's own text, not serde's marker.
    #[test]
    fn test_a_refused_key_is_named_as_written() {
        for (yaml, named) in [
            ("\"?\": true\n", "\"?\""),
            ("\"a.?.b\": true\n", "\"a.?.b\""),
            (
                "trusted_keys:\n  - key_id: x\n    \"?\": 1\n",
                "\"trusted_keys.0.?\"",
            ),
        ] {
            let error = TrustPolicy::from_yaml(yaml).unwrap_err().to_string();
            assert!(error.contains(named), "{yaml:?} must name {named}: {error}");
        }
    }

    #[test]
    fn test_all_trusted_key_ids() {
        let policy = TrustPolicy {
            require_signed: true,
            trusted_key_ids: vec!["sha256:aaa".to_string()],
            trusted_keys: vec![TrustedKey {
                key_id: "sha256:bbb".to_string(),
                public_key_path: None,
                name: None,
            }],
        };

        let ids = policy.all_trusted_key_ids();
        assert!(ids.contains("sha256:aaa"));
        assert!(ids.contains("sha256:bbb"));
    }

    #[test]
    fn test_permissive_policy() {
        let policy = TrustPolicy::permissive();
        assert!(!policy.require_signed);
        assert!(policy.is_key_trusted("sha256:anything"));
    }

    #[test]
    fn test_is_key_trusted() {
        let policy = TrustPolicy {
            require_signed: true,
            trusted_key_ids: vec!["sha256:trusted".to_string()],
            trusted_keys: vec![],
        };

        assert!(policy.is_key_trusted("sha256:trusted"));
        assert!(!policy.is_key_trusted("sha256:untrusted"));
    }
}
