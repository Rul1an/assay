use crate::lint::Severity;
use serde::Serialize;

assay_common::closed! {
    /// Pack kind determines validation rules and collision policy.
    #[derive(Copy, Eq, Serialize)]
    pub enum PackKind {
        /// Compliance packs require disclaimer and have strict collision handling.
        "compliance" => Compliance,
        /// Security packs have standard collision handling.
        "security" => Security,
        /// Quality packs have standard collision handling.
        "quality" => Quality,
    }
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

assay_common::closed! {
    /// Pack definition as loaded from YAML.
    #[derive(Serialize)]
    strict root pub struct PackDefinition {
        /// Pack identifier (lowercase alphanumeric + hyphens).
        name: String,

        /// Semver version string.
        version: String,

        /// Pack kind (compliance/security/quality).
        kind: PackKind,

        /// Human-readable description.
        description: String,

        /// Pack author name/org.
        author: String,

        /// SPDX license identifier.
        license: String,

        /// Primary source URL (e.g., EUR-Lex for EU regulations).
        #[default]
        source_url: Option<String>,

        /// Legal disclaimer (REQUIRED for compliance packs).
        #[default]
        disclaimer: Option<String>,

        /// Version requirements.
        requires: PackRequirements,

        /// Rule definitions.
        rules: Vec<PackRule>,
    }
}

assay_common::closed! {
    /// Version requirements for a pack.
    #[derive(Serialize)]
    strict part pub struct PackRequirements {
        /// Minimum Assay version (semver constraint, e.g., ">=2.9.0").
        assay_min_version: String,
        /// Evidence schema version (optional, for future compatibility).
        #[default]
        evidence_schema_version: Option<String>,
    }
}

assay_common::closed! {
    /// Rule definition within a pack.
    #[derive(Serialize)]
    strict part pub struct PackRule {
        /// Short rule ID (unique within pack).
        id: String,

        /// Rule severity.
        #[text_scalar]
        severity: Severity,

        /// One-line description.
        description: String,

        /// Regulatory reference (e.g., "12(1)").
        #[default]
        article_ref: Option<String>,

        /// Detailed help text with markdown.
        #[default]
        help_markdown: Option<String>,

        /// Check to perform.
        check: CheckDefinition,

        /// Minimum engine version required for this rule.
        /// Rules with unsupported check types should set this to a future version.
        #[default]
        engine_min_version: Option<String>,

        /// Event types this rule applies to (for filtering).
        #[default]
        event_types: Option<Vec<String>>,
    }
}

assay_common::closed! {
    /// Check definition (tagged union).
    ///
    /// Uses `#[serde(other)]` to capture unknown check types for forward compatibility.
    /// Unknown types will be parsed as `Unsupported` and handled based on `PackKind`.
    ///
    /// A known check refuses fields it does not read. Future types keep arbitrary payload fields;
    /// duplicate mapping keys are refused before that payload is discarded.
    strict pub enum CheckDefinition tagged "type" {
        /// Verify bundle contains minimum number of events.
        "event_count" => EventCount {
            /// Minimum event count required.
            min: usize,
        },

        /// Verify matching start/finish event pairs exist.
        "event_pairs" => EventPairs {
            /// Glob pattern for start events.
            start_pattern: String,
            /// Glob pattern for finish events.
            finish_pattern: String,
        },

        /// Verify at least one event contains specified fields.
        "event_field_present" => EventFieldPresent {
            /// JSON Pointer paths to check (RFC 6901).
            #[default]
            paths_any_of: Option<Vec<String>>,

            /// Legacy: field names to check.
            #[default]
            any_of: Option<Vec<String>>,

            /// Legacy: if true, check in data.* (default: false).
            #[default]
            in_data: bool,
        },

        /// Verify at least one event of specified type exists.
        "event_type_exists" => EventTypeExists {
            /// Glob pattern for event type.
            pattern: String,
        },

        /// Verify manifest contains specified field.
        "manifest_field" => ManifestField {
            /// JSON Pointer to field.
            path: String,
            /// If true, missing = error; if false, missing = warning.
            #[default]
            required: bool,
        },

        /// JSON path existence check (simple version for mandate rules).
        "json_path_exists" => JsonPathExists {
            /// JSON Pointer paths to check.
            paths: Vec<String>,
            /// When set, at least one scoped event must have this **exact** JSON value at **each**
            /// path (after resolving the pointer). Omitted = legacy presence-only semantics
            /// (`null` is a valid value; only missing paths fail).
            ///
            /// If set, `paths` must contain exactly one pointer (bundle-wide “any of these paths equals”
            /// is intentionally unsupported for v1).
            #[opaque_json = "value_equals"]
            value_equals: Option<serde_json::Value>,
        },

        /// G3 v1 (domain-specific): same predicate as Trust Basis `authorization_context_visible` (`verified`).
        /// Not a generic pack-engine auth DSL — no parameters by design.
        #[empty("This check takes no parameters.")]
        "g3_authorization_context_present" => G3AuthorizationContextPresent,

        /// Conditional check.
        ///
        /// Engine v1.1 supports a narrow typed subset:
        /// - `condition.all` with `{ path, equals }` clauses
        /// - `then: { type: json_path_exists, paths: [single-path] }`
        ///
        /// Other conditional shapes remain unsupported and are handled according to
        /// pack kind (skip for security/quality, fail for compliance).
        "conditional" => Conditional {
            /// Condition definition (opaque for now).
            #[opaque_json = "condition"]
            condition: Option<serde_json::Value>,
            /// Check to run if condition is true.
            #[opaque_json = "then"]
            then_check: Option<serde_json::Value>,
        };
        future "unsupported" => Unsupported
    }
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
