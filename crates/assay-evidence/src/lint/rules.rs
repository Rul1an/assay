use super::{EventLocation, LintFinding, Severity};
use crate::types::EvidenceEvent;
use serde_json::Value;
use std::collections::BTreeMap;

type RuleCheck = for<'a> fn(&EvidenceEvent, &LintContext<'a>) -> Option<LintFinding>;

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
    pub check: RuleCheck,
}

/// Context passed to rule checks.
pub struct LintContext<'a> {
    pub line_number: usize, // 1-indexed
    pub seq: usize,
    pub bundle_index: &'a LintBundleIndex,
}

/// Bundle-wide structural index used by built-in lint rules.
///
/// Rules stay single-event callbacks, but they can consult this precomputed
/// view when a claim can only be checked against sibling records in the same
/// verified bundle. The index is intentionally structural: it reads pinned
/// schema fields and never searches prose/log text.
#[derive(Debug, Default)]
pub struct LintBundleIndex {
    enforcement_decisions: BTreeMap<EnforcementDecisionKey, Vec<EnforcementDecisionSummary>>,
}

impl LintBundleIndex {
    pub fn from_events(events: &[EvidenceEvent]) -> Self {
        let mut index = Self::default();
        for event in events {
            if let Some((key, decision)) = extract_enforcement_decision(event) {
                index
                    .enforcement_decisions
                    .entry(key)
                    .or_default()
                    .push(decision);
            }
        }
        index
    }

    fn enforcement_decisions_for(
        &self,
        key: &EnforcementDecisionKey,
    ) -> &[EnforcementDecisionSummary] {
        self.enforcement_decisions
            .get(key)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EnforcementDecisionKey {
    tool_name: String,
    target_digest: String,
}

#[derive(Debug)]
struct EnforcementDecisionSummary {
    decision: String,
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
    RuleDefinition {
        id: "ASSAY-W004",
        default_severity: Severity::Warn,
        description:
            "Observed enforcement marker is not supported by a bound enforcement decision record",
        help_uri: Some("https://docs.assay.dev/lint/ASSAY-W004"),
        tags: &["security", "enforcement", "attribution"],
        security_severity: Some("4.0"),
        check: check_enforcement_attribution_binding,
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

fn check_secret_in_subject(event: &EvidenceEvent, ctx: &LintContext<'_>) -> Option<LintFinding> {
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

fn check_pii_flag_consistency(event: &EvidenceEvent, ctx: &LintContext<'_>) -> Option<LintFinding> {
    if event.contains_pii && event.subject.as_deref().filter(|s| !s.is_empty()).is_some() {
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
    None
}

fn check_source_format(event: &EvidenceEvent, ctx: &LintContext<'_>) -> Option<LintFinding> {
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

fn check_secrets_flag_consistency(
    event: &EvidenceEvent,
    ctx: &LintContext<'_>,
) -> Option<LintFinding> {
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

fn check_enforcement_attribution_binding(
    event: &EvidenceEvent,
    ctx: &LintContext<'_>,
) -> Option<LintFinding> {
    let key = extract_proxy_refusal_marker(event)?;
    let decisions = ctx.bundle_index.enforcement_decisions_for(&key);

    if decisions.iter().any(|record| record.decision == "deny") {
        return None;
    }

    let message = if decisions.iter().any(|record| record.decision == "allow") {
        format!(
            "Observed response carries an enforcement marker for '{}' but is contradicted by a digest-bound allow record",
            key.tool_name
        )
    } else {
        format!(
            "Observed response carries an enforcement marker for '{}' but no digest-bound assay.enforcement_decision.v0 deny record was retained",
            key.tool_name
        )
    };

    Some(
        LintFinding::new(
            "ASSAY-W004",
            Severity::Warn,
            message,
            Some(EventLocation {
                seq: ctx.seq,
                line: ctx.line_number,
                event_type: Some(event.type_.clone()),
            }),
            vec![
                "security".into(),
                "enforcement".into(),
                "attribution".into(),
            ],
        )
        .with_help_uri("https://docs.assay.dev/lint/ASSAY-W004"),
    )
}

fn extract_enforcement_decision(
    event: &EvidenceEvent,
) -> Option<(EnforcementDecisionKey, EnforcementDecisionSummary)> {
    let payload = &event.payload;
    if payload.get("schema").and_then(Value::as_str) != Some("assay.enforcement_decision.v0") {
        return None;
    }

    let key = EnforcementDecisionKey {
        tool_name: non_empty(payload.pointer("/tool/name")?.as_str()?)?.to_string(),
        target_digest: non_empty(payload.pointer("/action/target_digest")?.as_str()?)?.to_string(),
    };
    let decision = non_empty(payload.get("decision")?.as_str()?)?.to_string();

    Some((key, EnforcementDecisionSummary { decision }))
}

fn extract_proxy_refusal_marker(event: &EvidenceEvent) -> Option<EnforcementDecisionKey> {
    let payload = &event.payload;
    let observed_response = payload.get("observed_response")?.as_str()?;
    let response: Value = serde_json::from_str(observed_response).ok()?;
    if response.pointer("/error/data/assay_proxy")?.as_str()? != "deny" {
        return None;
    }

    Some(EnforcementDecisionKey {
        tool_name: non_empty(payload.pointer("/call/tool_name")?.as_str()?)?.to_string(),
        target_digest: non_empty(payload.pointer("/call/target_digest")?.as_str()?)?.to_string(),
    })
}

fn non_empty(value: &str) -> Option<&str> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
