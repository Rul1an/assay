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

    /// Simple list of trusted key IDs (sha256:...).
    #[serde(default)]
    pub trusted_key_ids: Vec<String>,

    /// Detailed trusted keys with metadata.
    #[serde(default)]
    pub trusted_keys: Vec<TrustedKey>,
}

/// A trusted key with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedKey {
    /// SHA-256 of SPKI bytes: sha256:<hex>
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

    /// Parse trust policy from YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        serde_yaml::from_str(yaml).context("failed to parse trust policy YAML")
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
    pub fn is_key_trusted(&self, key_id: &str) -> bool {
        // If no keys are configured, trust all (permissive mode)
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

/// A loaded public key with metadata.
#[derive(Debug)]
pub struct LoadedKey {
    pub key_id: String,
    pub key: VerifyingKey,
    pub name: Option<String>,
}

/// Load a public key from SPKI PEM file.
pub fn load_public_key_pem(path: &Path) -> Result<VerifyingKey> {
    use pkcs8::DecodePublicKey;

    let pem = fs::read_to_string(path)
        .with_context(|| format!("failed to read public key: {}", path.display()))?;

    VerifyingKey::from_public_key_pem(&pem)
        .with_context(|| format!("failed to parse public key PEM: {}", path.display()))
}

/// Load a private key from PKCS#8 PEM file.
pub fn load_private_key_pem(path: &Path) -> Result<ed25519_dalek::SigningKey> {
    use pkcs8::DecodePrivateKey;

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
trusted_key_ids:
  - "sha256:abc123"
  - "sha256:def456"
trusted_keys:
  - key_id: "sha256:789xyz"
    name: "CI Key"
"#;

        let policy = TrustPolicy::from_yaml(yaml).unwrap();
        assert!(policy.require_signed);
        assert_eq!(policy.trusted_key_ids.len(), 2);
        assert_eq!(policy.trusted_keys.len(), 1);
        assert_eq!(policy.trusted_keys[0].name, Some("CI Key".to_string()));
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
