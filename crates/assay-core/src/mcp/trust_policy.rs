//! Trust policy for tool signature verification.
//!
//! Defines which signing keys are trusted for tool verification.

use anyhow::{Context, Result};
use assay_common::limits::{LimitKind, LimitReader};
use assay_common::trust_document as view;
use ed25519_dalek::VerifyingKey;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::Read;
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
        let mut content = String::new();
        fs::File::open(path)
            .and_then(|file| {
                LimitReader::new(file, view::MAX_BYTES, LimitKind::SourceBytes)
                    .read_to_string(&mut content)
            })
            .with_context(|| format!("failed to read trust policy: {}", path.display()))?;
        Self::from_yaml(&content)
    }

    /// Parse a trust policy file as `assay mcp tool verify` reads it.
    ///
    /// The file is read into the shared trust document's types, which hold only the keys the
    /// verifier acts on, and every other key refuses the policy, named by its path. serde's
    /// defaults would otherwise drop such a key without a word: `require_signd: true` left
    /// `require_signed` false, and an unsigned tool passed. `trusted_key_ids` is refused with the
    /// entry that does verify.
    pub fn from_yaml(yaml: &str) -> Result<Self> {
        let (require_signed, trusted_keys) =
            view::read(yaml, |key_id, public_key_path, name| TrustedKey {
                key_id,
                public_key_path,
                name,
            })
            .map_err(|error| match error {
                view::Error::UnknownKeys(keys) => anyhow::anyhow!("{}", refusal(&keys)),
                view::Error::Yaml(cause) => {
                    anyhow::Error::new(cause).context("failed to parse trust policy YAML")
                }
                view::Error::TooLarge(cause) => {
                    anyhow::Error::new(cause).context("failed to parse trust policy YAML")
                }
                _ => anyhow::anyhow!("failed to parse trust policy YAML: unsupported reader error"),
            })?;
        Ok(Self {
            require_signed,
            trusted_key_ids: Vec::new(),
            trusted_keys,
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
    fn trust_file_preserves_authored_strings() {
        let policy = TrustPolicy::from_yaml("trusted_keys: [{key_id: 0x10, name: 1.0}]").unwrap();
        assert_eq!(policy.trusted_keys[0].key_id, "0x10");
        assert_eq!(policy.trusted_keys[0].name.as_deref(), Some("1.0"));
    }

    #[test]
    fn invalid_trust_field_precedes_unread_keys() {
        let error = TrustPolicy::from_yaml("unknown: 42\nrequire_signed: [1]").unwrap_err();
        assert!(format!("{error:#}").contains("expected a boolean"));
    }

    #[test]
    fn repeated_unread_keys_are_reported_in_file_order() {
        let error = TrustPolicy::from_yaml("other: true\nnext: 1\nother: false").unwrap_err();
        assert_eq!(
            error.to_string(),
            refusal(&["other".into(), "next".into(), "other".into()])
        );
    }

    #[test]
    fn trust_policy_byte_limit_applies_to_text_and_files_before_decoding() {
        const LIMIT: usize = 16 * 1024 * 1024;
        let mut text = String::from("require_signed: true\n#");
        text.extend(std::iter::repeat_n('x', LIMIT - text.len()));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trust.yaml");
        fs::write(&path, &text).unwrap();
        assert!(TrustPolicy::from_yaml(&text).unwrap().require_signed);
        assert!(TrustPolicy::from_file(&path).unwrap().require_signed);
        text.push('x');
        fs::write(&path, &text).unwrap();
        for result in [TrustPolicy::from_yaml(&text), TrustPolicy::from_file(&path)] {
            let error = result.unwrap_err();
            assert!(format!("{error:#}").contains("16777216"), "{error:#}");
        }

        // A whole-file UTF-8 read would fail on the final byte before seeing the limit.
        let mut bytes = text.into_bytes();
        bytes[LIMIT] = 0xff;
        fs::write(&path, bytes).unwrap();
        let error = TrustPolicy::from_file(&path).unwrap_err();
        let io = error.downcast_ref::<std::io::Error>().unwrap();
        let exceeded = assay_common::limits::LimitExceeded::from_io(io).unwrap();
        assert_eq!(exceeded.kind, LimitKind::SourceBytes);
        assert_eq!(exceeded.limit, LIMIT as u64);
    }

    #[test]
    fn bad_trust_field_keeps_a_concrete_located_yaml_error() {
        let error = TrustPolicy::from_yaml("other: 1\nrequire_signed: [1]").unwrap_err();
        let cause = error.downcast_ref::<serde_yaml::Error>().unwrap();
        assert_eq!(cause.location().unwrap().line(), 2);
    }

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
            ("0x10: value\n", "\"0x10\""),
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

    /// The view is the public type minus the keys the verifier refuses. Every key the verifier reads
    /// is one the schema has, and the schema's one other key, `trusted_key_ids`, is refused with the
    /// entry that does verify. A field added to `TrustPolicy` or `TrustedKey` fails here until the
    /// verifier reads it or this test names its refusal.
    #[test]
    fn test_the_view_is_the_public_type_minus_the_refused_keys() {
        use crate::unread_keys::keys_of;
        use std::collections::BTreeSet;
        let set = |keys: Option<&'static [&'static str]>| -> BTreeSet<&'static str> {
            keys.expect("a struct with named fields")
                .iter()
                .copied()
                .collect()
        };
        let fields = |shape| -> BTreeSet<&'static str> {
            let assay_common::doc::Shape::Struct { fields, .. } = shape else {
                panic!("expected named fields")
            };
            fields.iter().map(|field| field.key).collect()
        };
        let (file_shape, key_shape) = view::field_shapes();
        let (public, read) = (set(keys_of::<TrustPolicy>()), fields(file_shape));
        assert!(read.is_subset(&public), "{read:?} not in {public:?}");
        let refused: Vec<&str> = public.difference(&read).copied().collect();
        assert_eq!(refused, ["trusted_key_ids"]);
        for key in refused {
            assert!(
                refusal(&[key.to_string()]).contains("list the key under trusted_keys"),
                "{key} is refused with its own reason"
            );
        }
        assert_eq!(set(keys_of::<TrustedKey>()), fields(key_shape));
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
