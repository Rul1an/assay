use crate::lint::Severity;
use serde::{Deserialize, Serialize};

/// Pack kind determines validation rules and collision policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackKind {
    /// Compliance packs require disclaimer and have strict collision handling.
    Compliance,
    /// Security packs have standard collision handling.
    Security,
    /// Quality packs have standard collision handling.
    Quality,
}

impl std::fmt::Display for PackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PackKind::Compliance => write!(f, "compliance"),
            PackKind::Security => write!(f, "security"),
            PackKind::Quality => write!(f, "quality"),
        }
    }
}

/// Pack definition as loaded from YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackDefinition {
    /// Pack identifier (lowercase alphanumeric + hyphens).
    pub name: String,

    /// Semver version string.
    pub version: String,

    /// Pack kind (compliance/security/quality).
    pub kind: PackKind,

    /// Human-readable description.
    pub description: String,

    /// Pack author name/org.
    pub author: String,

    /// SPDX license identifier.
    pub license: String,

    /// Primary source URL (e.g., EUR-Lex for EU regulations).
    #[serde(default)]
    pub source_url: Option<String>,

    /// Legal disclaimer (REQUIRED for compliance packs).
    #[serde(default)]
    pub disclaimer: Option<String>,

    /// Version requirements.
    pub requires: PackRequirements,

    /// Rule definitions.
    pub rules: Vec<PackRule>,
}

/// Version requirements for a pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRequirements {
    /// Minimum Assay version (semver constraint, e.g., ">=2.9.0").
    pub assay_min_version: String,

    /// Evidence schema version (optional, for future compatibility).
    #[serde(default)]
    pub evidence_schema_version: Option<String>,
}

/// Rule definition within a pack.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackRule {
    /// Short rule ID (unique within pack).
    pub id: String,

    /// Rule severity.
    #[serde(
        serialize_with = "super::serde::serialize_pack_severity",
        deserialize_with = "super::serde::deserialize_pack_severity"
    )]
    pub severity: Severity,

    /// One-line description.
    pub description: String,

    /// Regulatory reference (e.g., "12(1)").
    #[serde(default)]
    pub article_ref: Option<String>,

    /// Detailed help text with markdown.
    #[serde(default)]
    pub help_markdown: Option<String>,

    /// Check to perform.
    pub check: CheckDefinition,

    /// Minimum engine version required for this rule.
    /// Rules with unsupported check types should set this to a future version.
    #[serde(default)]
    pub engine_min_version: Option<String>,

    /// Event types this rule applies to (for filtering).
    #[serde(default)]
    pub event_types: Option<Vec<String>>,
}

/// Check definition (tagged union).
///
/// Uses `#[serde(other)]` to capture unknown check types for forward compatibility.
/// Unknown types will be parsed as `Unsupported` and handled based on `PackKind`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CheckDefinition {
    /// Verify bundle contains minimum number of events.
    EventCount {
        /// Minimum event count required.
        min: usize,
    },

    /// Verify matching start/finish event pairs exist.
    EventPairs {
        /// Glob pattern for start events.
        start_pattern: String,
        /// Glob pattern for finish events.
        finish_pattern: String,
    },

    /// Verify at least one event contains specified fields.
    EventFieldPresent {
        /// JSON Pointer paths to check (RFC 6901).
        #[serde(default)]
        paths_any_of: Option<Vec<String>>,

        /// Legacy: field names to check.
        #[serde(default)]
        any_of: Option<Vec<String>>,

        /// Legacy: if true, check in data.* (default: false).
        #[serde(default)]
        in_data: bool,
    },

    /// Verify at least one event of specified type exists.
    EventTypeExists {
        /// Glob pattern for event type.
        pattern: String,
    },

    /// Verify manifest contains specified field.
    ManifestField {
        /// JSON Pointer to field.
        path: String,
        /// If true, missing = error; if false, missing = warning.
        #[serde(default)]
        required: bool,
    },

    /// JSON path existence check (simple version for mandate rules).
    JsonPathExists {
        /// JSON Pointer paths to check.
        paths: Vec<String>,
        /// When set, at least one scoped event must have this **exact** JSON value at **each**
        /// path (after resolving the pointer). Omitted = legacy presence-only semantics
        /// (`null` is a valid value; only missing paths fail).
        ///
        /// If set, `paths` must contain exactly one pointer (bundle-wide “any of these paths equals”
        /// is intentionally unsupported for v1).
        #[serde(default)]
        value_equals: Option<serde_json::Value>,
    },

    /// G3 v1 (domain-specific): same predicate as Trust Basis `authorization_context_visible` (`verified`).
    /// Not a generic pack-engine auth DSL — no parameters by design.
    #[serde(rename = "g3_authorization_context_present")]
    G3AuthorizationContextPresent,

    /// Conditional check.
    ///
    /// Engine v1.1 supports a narrow typed subset:
    /// - `condition.all` with `{ path, equals }` clauses
    /// - `then: { type: json_path_exists, paths: [single-path] }`
    ///
    /// Other conditional shapes remain unsupported and are handled according to
    /// pack kind (skip for security/quality, fail for compliance).
    #[serde(rename = "conditional")]
    Conditional {
        /// Condition definition (opaque for now).
        #[serde(default)]
        condition: Option<serde_json::Value>,
        /// Check to run if condition is true.
        #[serde(default, rename = "then")]
        then_check: Option<serde_json::Value>,
    },

    /// Unknown check type - forward compatibility.
    /// Captured when deserializing unknown check types.
    #[serde(other)]
    Unsupported,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportedConditionalCheck {
    pub clauses: Vec<SupportedConditionalClause>,
    pub required_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SupportedConditionalClause {
    pub path: String,
    pub equals: serde_json::Value,
}
