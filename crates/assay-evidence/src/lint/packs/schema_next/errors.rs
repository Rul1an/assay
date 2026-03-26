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

    #[error("Pack safety check failed: {0}")]
    Safety(String),
}
