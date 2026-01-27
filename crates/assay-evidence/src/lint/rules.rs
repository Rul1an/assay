use super::{EventLocation, LintFinding, Severity};
use crate::types::EvidenceEvent;

/// Rule definition for the lint registry.
pub struct RuleDefinition {
    pub id: &'static str,
    pub default_severity: Severity,
    pub description: &'static str,
    pub help_uri: Option<&'static str>,
    pub tags: &'static [&'static str],
    /// CVSS-like security severity (0.0–10.0) for GitHub Code Scanning integration.
    /// Only set for security-relevant rules.
    pub security_severity: Option<&'static str>,
    pub check: fn(&EvidenceEvent, &LintContext) -> Option<LintFinding>,
}

/// Context passed to rule checks.
pub struct LintContext {
    pub line_number: usize, // 1-indexed
    pub seq: usize,
}

/// Static rule registry.
pub static RULES: &[RuleDefinition] = &[
    RuleDefinition {
        id: "ASSAY-W001",
        default_severity: Severity::Warn,
        description: "Subject may contain a secret (API key, token, password pattern)",
        help_uri: Some("https://docs.assay.dev/lint/ASSAY-W001"),
        tags: &["security", "secrets"],
        security_severity: Some("7.0"),
        check: check_secret_in_subject,
    },
    RuleDefinition {
        id: "ASSAY-W002",
        default_severity: Severity::Warn,
        description: "Event flagged as containing PII but subject is non-empty",
        help_uri: Some("https://docs.assay.dev/lint/ASSAY-W002"),
        tags: &["privacy", "pii"],
        security_severity: Some("4.0"),
        check: check_pii_flag_consistency,
    },
    RuleDefinition {
        id: "ASSAY-I001",
        default_severity: Severity::Info,
        description: "Source format does not follow URN convention",
        help_uri: Some("https://docs.assay.dev/lint/ASSAY-I001"),
        tags: &["convention", "format"],
        security_severity: None,
        check: check_source_format,
    },
    RuleDefinition {
        id: "ASSAY-W003",
        default_severity: Severity::Warn,
        description: "Event flagged as containing secrets but secrets flag is false",
        help_uri: Some("https://docs.assay.dev/lint/ASSAY-W003"),
        tags: &["security", "secrets"],
        security_severity: Some("6.5"),
        check: check_secrets_flag_consistency,
    },
];

/// Patterns that suggest secrets in subjects.
const SECRET_PATTERNS: &[&str] = &[
    "sk-",
    "sk_live_",
    "sk_test_",
    "api_key=",
    "apikey=",
    "token=",
    "password=",
    "secret=",
    "Bearer ",
    "AKIA", // AWS access key prefix
    "ghp_", // GitHub personal access token
    "gho_", // GitHub OAuth token
    "github_pat_",
];

fn check_secret_in_subject(event: &EvidenceEvent, ctx: &LintContext) -> Option<LintFinding> {
    let subject = event.subject.as_deref()?;
    let lower = subject.to_lowercase();

    for pattern in SECRET_PATTERNS {
        if lower.contains(&pattern.to_lowercase()) {
            return Some(
                LintFinding::new(
                    "ASSAY-W001",
                    Severity::Warn,
                    format!(
                        "Subject may contain a secret (matched pattern '{}')",
                        pattern
                    ),
                    Some(EventLocation {
                        seq: ctx.seq,
                        line: ctx.line_number,
                        event_type: Some(event.type_.clone()),
                    }),
                    vec!["security".into(), "secrets".into()],
                )
                .with_help_uri("https://docs.assay.dev/lint/ASSAY-W001"),
            );
        }
    }
    None
}

fn check_pii_flag_consistency(event: &EvidenceEvent, ctx: &LintContext) -> Option<LintFinding> {
    if event.contains_pii && event.subject.is_some() {
        let subject = event.subject.as_deref().unwrap_or("");
        // If PII flag is set and subject is non-empty, warn about potential PII leak in subject
        if !subject.is_empty() {
            return Some(
                LintFinding::new(
                    "ASSAY-W002",
                    Severity::Warn,
                    "Event marked as containing PII has a non-empty subject — consider redacting",
                    Some(EventLocation {
                        seq: ctx.seq,
                        line: ctx.line_number,
                        event_type: Some(event.type_.clone()),
                    }),
                    vec!["privacy".into(), "pii".into()],
                )
                .with_help_uri("https://docs.assay.dev/lint/ASSAY-W002"),
            );
        }
    }
    None
}

fn check_source_format(event: &EvidenceEvent, ctx: &LintContext) -> Option<LintFinding> {
    // Convention: source should be urn: or https:// scheme
    if !event.source.starts_with("urn:") && !event.source.starts_with("https://") {
        return Some(
            LintFinding::new(
                "ASSAY-I001",
                Severity::Info,
                format!(
                    "Source '{}' does not follow urn: or https:// convention",
                    event.source
                ),
                Some(EventLocation {
                    seq: ctx.seq,
                    line: ctx.line_number,
                    event_type: Some(event.type_.clone()),
                }),
                vec!["convention".into(), "format".into()],
            )
            .with_help_uri("https://docs.assay.dev/lint/ASSAY-I001"),
        );
    }
    None
}

fn check_secrets_flag_consistency(event: &EvidenceEvent, ctx: &LintContext) -> Option<LintFinding> {
    // If subject contains a secret pattern but contains_secrets is false, warn
    if let Some(subject) = &event.subject {
        let lower = subject.to_lowercase();
        let has_secret_pattern = SECRET_PATTERNS
            .iter()
            .any(|p| lower.contains(&p.to_lowercase()));

        if has_secret_pattern && !event.contains_secrets {
            return Some(
                LintFinding::new(
                    "ASSAY-W003",
                    Severity::Warn,
                    "Subject appears to contain a secret but contains_secrets flag is false",
                    Some(EventLocation {
                        seq: ctx.seq,
                        line: ctx.line_number,
                        event_type: Some(event.type_.clone()),
                    }),
                    vec!["security".into(), "secrets".into()],
                )
                .with_help_uri("https://docs.assay.dev/lint/ASSAY-W003"),
            );
        }
    }
    None
}
