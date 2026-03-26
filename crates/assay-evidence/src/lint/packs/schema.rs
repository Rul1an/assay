//! Pack schema types.
//!
//! Defines the structure of compliance/security/quality packs per SPEC-Pack-Engine-v1.

#[path = "schema_next/mod.rs"]
mod schema_next;

pub use crate::lint::Severity;
pub use schema_next::{
    is_valid_pack_name, CheckDefinition, PackDefinition, PackKind, PackRequirements, PackRule,
    PackValidationError, SupportedConditionalCheck, SupportedConditionalClause,
};

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

    #[test]
    fn test_supported_conditional_shape_parses() {
        let check = CheckDefinition::Conditional {
            condition: Some(serde_json::json!({
                "all": [
                    {
                        "path": "/data/decision",
                        "equals": "allow"
                    }
                ]
            })),
            then_check: Some(serde_json::json!({
                "type": "json_path_exists",
                "paths": ["/data/mandate_id"]
            })),
        };

        let conditional = check
            .supported_conditional()
            .expect("conditional subset should parse");
        assert_eq!(conditional.clauses.len(), 1);
        assert_eq!(conditional.clauses[0].path, "/data/decision");
        assert_eq!(conditional.clauses[0].equals, serde_json::json!("allow"));
        assert_eq!(conditional.required_path, "/data/mandate_id");
        assert!(!check.is_unsupported());
    }

    #[test]
    fn test_conditional_with_multiple_then_paths_is_unsupported() {
        let check = CheckDefinition::Conditional {
            condition: Some(serde_json::json!({
                "all": [
                    {
                        "path": "/data/decision",
                        "equals": "allow"
                    }
                ]
            })),
            then_check: Some(serde_json::json!({
                "type": "json_path_exists",
                "paths": ["/data/mandate_id", "/data/approval_state"]
            })),
        };

        let error = check
            .supported_conditional()
            .expect_err("multiple then paths should remain unsupported");
        assert!(error.contains("exactly one required path"));
        assert!(check.is_unsupported());
    }

    #[test]
    fn test_conditional_validation_requires_condition_object() {
        let pack = PackDefinition {
            name: "conditional-pack".to_string(),
            version: "1.0.0".to_string(),
            kind: PackKind::Security,
            description: "test".to_string(),
            author: "Assay Team".to_string(),
            license: "Apache-2.0".to_string(),
            source_url: None,
            disclaimer: None,
            requires: PackRequirements {
                assay_min_version: ">=0.0.0".to_string(),
                evidence_schema_version: None,
            },
            rules: vec![PackRule {
                id: "COND-001".to_string(),
                severity: Severity::Error,
                description: "test".to_string(),
                article_ref: None,
                help_markdown: None,
                check: CheckDefinition::Conditional {
                    condition: None,
                    then_check: Some(serde_json::json!({
                        "type": "json_path_exists",
                        "paths": ["/data/mandate_id"]
                    })),
                },
                engine_min_version: None,
                event_types: None,
            }],
        };

        let error = pack
            .validate()
            .expect_err("missing conditional condition should fail validation");
        assert!(matches!(
            error,
            PackValidationError::InvalidCheck { reason, .. }
                if reason == "conditional requires a condition object"
        ));
    }

    #[test]
    fn test_conditional_validation_requires_then_object() {
        let pack = PackDefinition {
            name: "conditional-pack".to_string(),
            version: "1.0.0".to_string(),
            kind: PackKind::Security,
            description: "test".to_string(),
            author: "Assay Team".to_string(),
            license: "Apache-2.0".to_string(),
            source_url: None,
            disclaimer: None,
            requires: PackRequirements {
                assay_min_version: ">=0.0.0".to_string(),
                evidence_schema_version: None,
            },
            rules: vec![PackRule {
                id: "COND-001".to_string(),
                severity: Severity::Error,
                description: "test".to_string(),
                article_ref: None,
                help_markdown: None,
                check: CheckDefinition::Conditional {
                    condition: Some(serde_json::json!({
                        "all": [
                            {
                                "path": "/data/decision",
                                "equals": "allow"
                            }
                        ]
                    })),
                    then_check: None,
                },
                engine_min_version: None,
                event_types: None,
            }],
        };

        let error = pack
            .validate()
            .expect_err("missing conditional then should fail validation");
        assert!(matches!(
            error,
            PackValidationError::InvalidCheck { reason, .. }
                if reason == "conditional requires a then object"
        ));
    }

    #[test]
    fn test_json_path_exists_value_equals_requires_exactly_one_path() {
        let pack = PackDefinition {
            name: "jp-pack".to_string(),
            version: "1.0.0".to_string(),
            kind: PackKind::Security,
            description: "test".to_string(),
            author: "Assay Team".to_string(),
            license: "Apache-2.0".to_string(),
            source_url: None,
            disclaimer: None,
            requires: PackRequirements {
                assay_min_version: ">=0.0.0".to_string(),
                evidence_schema_version: None,
            },
            rules: vec![PackRule {
                id: "JP-001".to_string(),
                severity: Severity::Error,
                description: "test".to_string(),
                article_ref: None,
                help_markdown: None,
                check: CheckDefinition::JsonPathExists {
                    paths: vec!["/data/a".into(), "/data/b".into()],
                    value_equals: Some(serde_json::json!(true)),
                },
                engine_min_version: None,
                event_types: None,
            }],
        };

        let error = pack
            .validate()
            .expect_err("value_equals with two paths should fail validation");
        assert!(matches!(
            error,
            PackValidationError::InvalidCheck { reason, .. }
                if reason == "json_path_exists.value_equals requires exactly one path"
        ));
    }
}
