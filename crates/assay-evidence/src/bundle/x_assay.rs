//! x-assay manifest extension (ADR-025 E2).
//!
//! Producer vs consumer split: bundle_provenance (producer), evaluations (sidecar).
//! Schema version x-assay-ext-v1.
//!
//! ## Forward-compat: extensions vs extra
//!
//! - **extensions:** Use for future keys that are part of the contract (tooling may interpret).
//!   Prefer `x-assay.extensions.<key>` for new extension points.
//! - **extra** (flatten): Unknown top-level keys under x-assay land here on deserialize.
//!   Preserved on roundtrip only; tooling MAY ignore. Never put the same semantic in both.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Schema version string for x-assay-ext-v1.
pub const X_ASSAY_EXT_V1: &str = "x-assay-ext-v1";

/// Alias for clarity in tests/docs.
pub const SCHEMA_V1: &str = X_ASSAY_EXT_V1;

/// Input for building bundle_provenance at write time (ADR-025 E2 Phase 2).
#[derive(Debug, Clone)]
pub struct ProvenanceInput {
    pub producer_name: String,
    pub producer_version: String,
    pub git_commit: Option<String>,
    pub dirty: Option<bool>,
    pub run_id: String,
    /// RFC3339 UTC. If None, writer uses first event time (deterministic).
    pub created_at: Option<String>,
}

impl ProvenanceInput {
    /// Build XAssayExtension with bundle_provenance. Use `bundle_digest` placeholder
    /// during digest computation; caller substitutes real digest for final manifest.
    pub fn build_x_assay(&self, bundle_digest: &str, created_at: &str) -> XAssayExtension {
        let producer = ProducerInfo {
            name: self.producer_name.clone(),
            version: self.producer_version.clone(),
            build: (self.git_commit.is_some() || self.dirty.is_some()).then(|| BuildMeta {
                git_commit: self.git_commit.clone(),
                dirty: self.dirty,
            }),
        };
        let evidence = EvidenceMeta {
            schema_version: Some("evidence-bundle-v1".into()),
            format: Some("tar.gz".into()),
            bundle_digest: Some(bundle_digest.to_string()),
        };
        let run = RunMeta {
            run_id: Some(self.run_id.clone()),
            assayrunid: None,
        };
        let bundle_provenance = BundleProvenance {
            created_at: Some(created_at.to_string()),
            producer: Some(producer),
            evidence: Some(evidence),
            run: Some(run),
            model: None,
            source: None,
            environment: None,
        };
        XAssayExtension {
            schema_version: X_ASSAY_EXT_V1.to_string(),
            bundle_finalized: Some(true),
            bundle_provenance: Some(bundle_provenance),
            extensions: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }
}

/// x-assay extension container. Present when manifest has "x-assay" key.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct XAssayExtension {
    /// Contract version: "x-assay-ext-v1"
    pub schema_version: String,

    /// When true, bundle is immutable (exported/created). Producers MUST set on export.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_finalized: Option<bool>,

    /// Producer-side provenance (toolchain, model, run_id).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_provenance: Option<BundleProvenance>,

    /// Forward-compat: unknown keys preserved on roundtrip.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extensions: BTreeMap<String, serde_json::Value>,

    /// Unknown top-level x-assay keys preserved on roundtrip (ADR-025 E2 Phase 1).
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl XAssayExtension {
    /// Reserved keys that MUST NOT appear in `extra`, since `extra` is flattened
    /// at the same level and would collide on serialize.
    const RESERVED_TOP_LEVEL_KEYS: &'static [&'static str] = &[
        "schema_version",
        "bundle_finalized",
        "bundle_provenance",
        "extensions",
        "extra",
    ];

    /// Phase 1 safety invariants: roundtrip + deterministic serialization.
    /// Call before writing manifest.json. Phase 2+ semantic checks live in validate_semantics().
    pub fn validate_safety(&self) -> Result<()> {
        if self.schema_version != X_ASSAY_EXT_V1 {
            bail!(
                "x-assay.schema_version must be {:?} (got {:?})",
                X_ASSAY_EXT_V1,
                self.schema_version
            );
        }
        for k in self.extra.keys() {
            if Self::RESERVED_TOP_LEVEL_KEYS.contains(&k.as_str()) {
                bail!(
                    "x-assay.extra contains reserved key {:?} (would collide on serialize)",
                    k
                );
            }
        }
        for k in self.extensions.keys() {
            if k.trim().is_empty() {
                bail!("x-assay.extensions contains an empty key");
            }
        }
        Ok(())
    }
}

/// Producer provenance (immutable once bundle final).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BundleProvenance {
    /// RFC3339 UTC
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// Producer identity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub producer: Option<ProducerInfo>,

    /// Evidence schema and bundle digest
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<EvidenceMeta>,

    /// Run identity
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<RunMeta>,

    /// Model identity (no secrets)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelMeta>,

    /// Source (repo, git ref)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceMeta>,

    /// Environment (CI, runner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<EnvironmentMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProducerInfo {
    pub name: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildMeta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BuildMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_commit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// sha256:hex
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bundle_digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assayrunid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// sha256:hex over canonical config
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnvironmentMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_arch: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_x_assay_roundtrip_preserves_extensions_and_extra() {
        let json = r#"{
            "schema_version": "x-assay-ext-v1",
            "extensions": {"future_field": "value"},
            "future_key": 42
        }"#;
        let parsed: XAssayExtension = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.schema_version, X_ASSAY_EXT_V1);
        assert_eq!(parsed.extensions.get("future_field").unwrap(), "value");
        assert_eq!(parsed.extra.get("future_key").unwrap(), 42);

        let serialized = serde_json::to_string(&parsed).unwrap();
        let parsed2: XAssayExtension = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, parsed2);
        assert_eq!(parsed2.extra.get("future_key").unwrap(), 42);
    }

    #[test]
    fn test_x_assay_rejects_unknown_schema_version() {
        let x = XAssayExtension {
            schema_version: "x-assay-ext-vl".into(), // typo
            bundle_finalized: None,
            bundle_provenance: None,
            extensions: BTreeMap::new(),
            extra: BTreeMap::new(),
        };
        let err = x.validate_safety().unwrap_err().to_string();
        assert!(err.contains("schema_version"));
        assert!(err.contains("x-assay-ext-v1"));
    }

    #[test]
    fn test_x_assay_rejects_reserved_keys_in_extra() {
        let mut x = XAssayExtension {
            schema_version: X_ASSAY_EXT_V1.to_string(),
            bundle_finalized: None,
            bundle_provenance: None,
            extensions: BTreeMap::new(),
            extra: BTreeMap::new(),
        };
        x.extra
            .insert("schema_version".to_string(), serde_json::json!("evil"));

        let err = x.validate_safety().unwrap_err().to_string();
        assert!(err.contains("reserved key"));
        assert!(err.contains("schema_version"));
    }

    #[test]
    fn test_x_assay_rejects_empty_extension_key() {
        let mut x = XAssayExtension {
            schema_version: X_ASSAY_EXT_V1.to_string(),
            bundle_finalized: None,
            bundle_provenance: None,
            extensions: BTreeMap::new(),
            extra: BTreeMap::new(),
        };
        x.extensions.insert("".to_string(), serde_json::json!(1));

        let err = x.validate_safety().unwrap_err().to_string();
        assert!(err.contains("empty key"));
    }

    #[test]
    fn test_x_assay_roundtrip_preserves_nested_structures() {
        // Deep nested array/object in extensions to avoid Value→string coercion bugs
        let json = r#"{
            "schema_version": "x-assay-ext-v1",
            "extensions": {
                "nested": {"a": [1, {"b": 2}], "deep": [{"x": [true, false]}]}
            }
        }"#;
        let parsed: XAssayExtension = serde_json::from_str(json).unwrap();
        let nested = parsed.extensions.get("nested").unwrap();
        assert!(nested.get("a").is_some());
        assert_eq!(
            nested.get("a").unwrap().get(1).unwrap().get("b").unwrap(),
            2
        );

        let serialized = serde_json::to_string(&parsed).unwrap();
        let parsed2: XAssayExtension = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed, parsed2);
    }
}
