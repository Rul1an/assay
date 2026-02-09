//! Pack schema types.
//!
//! Defines the structure of compliance/security/quality packs per SPEC-Pack-Engine-v1.

pub use crate::lint::Severity;
use serde::de::Error as _;
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

fn serialize_pack_severity<S>(severity: &Severity, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let value = match severity {
        Severity::Error => "error",
        Severity::Warn => "warning",
        Severity::Info => "info",
    };
    serializer.serialize_str(value)
}

fn deserialize_pack_severity<'de, D>(deserializer: D) -> Result<Severity, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw = String::deserialize(deserializer)?;
    match raw.as_str() {
        "error" | "Error" => Ok(Severity::Error),
        "warning" | "Warning" | "warn" | "Warn" => Ok(Severity::Warn),
        "info" | "Info" => Ok(Severity::Info),
        _ => Err(D::Error::custom(format!(
            "invalid severity '{}'; expected error|warning|info",
            raw
        ))),
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

impl PackDefinition {
    /// Validate the pack definition.
    pub fn validate(&self) -> Result<(), PackValidationError> {
        // Compliance packs MUST have disclaimer
        if self.kind == PackKind::Compliance && self.disclaimer.is_none() {
            return Err(PackValidationError::MissingDisclaimer {
                pack: self.name.clone(),
            });
        }

        // Validate pack name format
        if !is_valid_pack_name(&self.name) {
            return Err(PackValidationError::InvalidPackName {
                name: self.name.clone(),
            });
        }

        // Validate rules
        let mut seen_ids = std::collections::HashSet::new();
        for rule in &self.rules {
            if !seen_ids.insert(&rule.id) {
                return Err(PackValidationError::DuplicateRuleId {
                    pack: self.name.clone(),
                    rule_id: rule.id.clone(),
                });
            }
            rule.validate(&self.name)?;
        }

        Ok(())
    }
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
        serialize_with = "serialize_pack_severity",
        deserialize_with = "deserialize_pack_severity"
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

impl PackRule {
    /// Validate the rule definition.
    pub fn validate(&self, pack_name: &str) -> Result<(), PackValidationError> {
        if self.id.is_empty() {
            return Err(PackValidationError::EmptyRuleId {
                pack: pack_name.to_string(),
            });
        }
        self.check.validate(pack_name, &self.id)?;
        Ok(())
    }

    /// Get the canonical rule ID: {pack}@{version}:{rule_id}
    pub fn canonical_id(&self, pack_name: &str, pack_version: &str) -> String {
        format!("{}@{}:{}", pack_name, pack_version, self.id)
    }
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
    },

    /// Conditional check (requires engine v1.1).
    /// Captured but not executed in current engine version.
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

impl CheckDefinition {
    /// Validate the check definition.
    pub fn validate(&self, pack_name: &str, rule_id: &str) -> Result<(), PackValidationError> {
        match self {
            CheckDefinition::EventCount { min } => {
                if *min == 0 {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "event_count.min must be > 0".to_string(),
                    });
                }
            }
            CheckDefinition::EventPairs {
                start_pattern,
                finish_pattern,
            } => {
                if start_pattern.is_empty() || finish_pattern.is_empty() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "event_pairs patterns cannot be empty".to_string(),
                    });
                }
            }
            CheckDefinition::EventFieldPresent {
                paths_any_of,
                any_of,
                ..
            } => {
                let has_paths = paths_any_of.as_ref().is_some_and(|p| !p.is_empty());
                let has_legacy = any_of.as_ref().is_some_and(|a| !a.is_empty());
                if !has_paths && !has_legacy {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "event_field_present requires paths_any_of or any_of".to_string(),
                    });
                }
            }
            CheckDefinition::EventTypeExists { pattern } => {
                if pattern.is_empty() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "event_type_exists.pattern cannot be empty".to_string(),
                    });
                }
            }
            CheckDefinition::ManifestField { path, .. } => {
                if path.is_empty() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "manifest_field.path cannot be empty".to_string(),
                    });
                }
            }
            CheckDefinition::JsonPathExists { paths } => {
                if paths.is_empty() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "json_path_exists.paths cannot be empty".to_string(),
                    });
                }
            }
            CheckDefinition::Conditional { .. } => {
                // Conditional checks are captured but validation is deferred to execution
                // (requires engine v1.1)
            }
            CheckDefinition::Unsupported => {
                // Unknown check types - validation handled by engine version check
            }
        }
        Ok(())
    }

    /// Check if this is an unsupported/future check type.
    pub fn is_unsupported(&self) -> bool {
        matches!(
            self,
            CheckDefinition::Unsupported | CheckDefinition::Conditional { .. }
        )
    }

    /// Get the check type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            CheckDefinition::EventCount { .. } => "event_count",
            CheckDefinition::EventPairs { .. } => "event_pairs",
            CheckDefinition::EventFieldPresent { .. } => "event_field_present",
            CheckDefinition::EventTypeExists { .. } => "event_type_exists",
            CheckDefinition::ManifestField { .. } => "manifest_field",
            CheckDefinition::JsonPathExists { .. } => "json_path_exists",
            CheckDefinition::Conditional { .. } => "conditional",
            CheckDefinition::Unsupported => "unsupported",
        }
    }

    /// Get normalized JSON Pointer paths for EventFieldPresent.
    pub fn get_field_paths(&self) -> Vec<String> {
        match self {
            CheckDefinition::EventFieldPresent {
                paths_any_of,
                any_of,
                in_data,
            } => {
                // Prefer paths_any_of if present
                if let Some(paths) = paths_any_of {
                    if !paths.is_empty() {
                        return paths.clone();
                    }
                }

                // Fall back to legacy any_of + in_data
                if let Some(fields) = any_of {
                    return fields
                        .iter()
                        .map(|f| {
                            if *in_data {
                                format!("/data/{}", f)
                            } else {
                                format!("/{}", f)
                            }
                        })
                        .collect();
                }

                vec![]
            }
            _ => vec![],
        }
    }
}

/// Pack validation error.
#[derive(Debug, thiserror::Error)]
pub enum PackValidationError {
    #[error("Pack '{pack}' is kind 'compliance' but missing 'disclaimer'")]
    MissingDisclaimer { pack: String },

    #[error("Invalid pack name '{name}': must be lowercase alphanumeric with hyphens")]
    InvalidPackName { name: String },

    #[error("Pack '{pack}' has duplicate rule ID '{rule_id}'")]
    DuplicateRuleId { pack: String, rule_id: String },

    #[error("Pack '{pack}' has empty rule ID")]
    EmptyRuleId { pack: String },

    #[error("Pack '{pack}' rule '{rule}' has invalid check: {reason}")]
    InvalidCheck {
        pack: String,
        rule: String,
        reason: String,
    },
}

/// Check if a pack name is valid (lowercase alphanumeric + hyphens).
fn is_valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_pack_names() {
        assert!(is_valid_pack_name("eu-ai-act-baseline"));
        assert!(is_valid_pack_name("soc2"));
        assert!(is_valid_pack_name("pack-v1"));
        assert!(is_valid_pack_name("a"));
    }

    #[test]
    fn test_invalid_pack_names() {
        assert!(!is_valid_pack_name(""));
        assert!(!is_valid_pack_name("-pack"));
        assert!(!is_valid_pack_name("pack-"));
        assert!(!is_valid_pack_name("Pack"));
        assert!(!is_valid_pack_name("pack_name"));
        assert!(!is_valid_pack_name("pack name"));
    }

    #[test]
    fn test_severity_priority() {
        assert!(Severity::Info.priority() < Severity::Warn.priority());
        assert!(Severity::Warn.priority() < Severity::Error.priority());
    }

    #[test]
    fn test_get_field_paths_modern() {
        let check = CheckDefinition::EventFieldPresent {
            paths_any_of: Some(vec!["/run_id".into(), "/data/traceparent".into()]),
            any_of: None,
            in_data: false,
        };
        assert_eq!(
            check.get_field_paths(),
            vec!["/run_id", "/data/traceparent"]
        );
    }

    #[test]
    fn test_get_field_paths_legacy() {
        let check = CheckDefinition::EventFieldPresent {
            paths_any_of: None,
            any_of: Some(vec!["traceparent".into()]),
            in_data: true,
        };
        assert_eq!(check.get_field_paths(), vec!["/data/traceparent"]);
    }
}
