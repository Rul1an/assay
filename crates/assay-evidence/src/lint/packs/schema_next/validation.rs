use super::errors::PackValidationError;
use super::types::{CheckDefinition, PackDefinition, PackKind, PackRule};
use std::collections::HashSet;

impl PackDefinition {
    /// Validate the pack definition.
    pub fn validate(&self) -> Result<(), PackValidationError> {
        if self.kind == PackKind::Compliance && self.disclaimer.is_none() {
            return Err(PackValidationError::MissingDisclaimer {
                pack: self.name.clone(),
            });
        }

        if !is_valid_pack_name(&self.name) {
            return Err(PackValidationError::InvalidPackName {
                name: self.name.clone(),
            });
        }

        let mut seen_ids = HashSet::new();
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

impl PackRule {
    /// Validate the rule definition.
    pub fn validate(&self, pack_name: &str) -> Result<(), PackValidationError> {
        if self.id.is_empty() {
            return Err(PackValidationError::EmptyRuleId {
                pack: pack_name.to_string(),
            });
        }
        if let Some(event_types) = &self.event_types {
            if event_types.is_empty() || event_types.iter().any(|event_type| event_type.is_empty())
            {
                return Err(PackValidationError::InvalidCheck {
                    pack: pack_name.to_string(),
                    rule: self.id.clone(),
                    reason: "event_types must contain at least one non-empty event type"
                        .to_string(),
                });
            }
        }
        self.check.validate(pack_name, &self.id)?;
        Ok(())
    }

    /// Get the canonical rule ID: {pack}@{version}:{rule_id}
    pub fn canonical_id(&self, pack_name: &str, pack_version: &str) -> String {
        format!("{}@{}:{}", pack_name, pack_version, self.id)
    }
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
            CheckDefinition::JsonPathExists {
                paths,
                value_equals,
            } => {
                if paths.is_empty() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "json_path_exists.paths cannot be empty".to_string(),
                    });
                }
                if value_equals.is_some() && paths.len() != 1 {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "json_path_exists.value_equals requires exactly one path"
                            .to_string(),
                    });
                }
            }
            CheckDefinition::G3AuthorizationContextPresent => {}
            CheckDefinition::Conditional {
                condition,
                then_check,
            } => {
                let condition =
                    condition
                        .as_ref()
                        .ok_or_else(|| PackValidationError::InvalidCheck {
                            pack: pack_name.to_string(),
                            rule: rule_id.to_string(),
                            reason: "conditional requires a condition object".to_string(),
                        })?;
                if !condition.is_object() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "conditional.condition must be an object".to_string(),
                    });
                }

                let then_check =
                    then_check
                        .as_ref()
                        .ok_or_else(|| PackValidationError::InvalidCheck {
                            pack: pack_name.to_string(),
                            rule: rule_id.to_string(),
                            reason: "conditional requires a then object".to_string(),
                        })?;
                if !then_check.is_object() {
                    return Err(PackValidationError::InvalidCheck {
                        pack: pack_name.to_string(),
                        rule: rule_id.to_string(),
                        reason: "conditional.then must be an object".to_string(),
                    });
                }
            }
            CheckDefinition::Unsupported => {}
        }
        Ok(())
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
            CheckDefinition::G3AuthorizationContextPresent => "g3_authorization_context_present",
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
                if let Some(paths) = paths_any_of {
                    if !paths.is_empty() {
                        return paths.clone();
                    }
                }

                if let Some(fields) = any_of {
                    return fields
                        .iter()
                        .map(|field| {
                            if *in_data {
                                format!("/data/{field}")
                            } else {
                                format!("/{field}")
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

/// Check if a pack name is valid (lowercase alphanumeric + hyphens).
/// Validate pack name grammar per ADR-021.
pub fn is_valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
}
