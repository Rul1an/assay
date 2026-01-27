pub mod engine;
pub mod rules;
pub mod sarif;

use crate::bundle::writer::Manifest;
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warn,
    Info,
}

impl Severity {
    pub fn as_sarif_level(&self) -> &str {
        match self {
            Severity::Error => "error",
            Severity::Warn => "warning",
            Severity::Info => "note",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Severity::Error => write!(f, "error"),
            Severity::Warn => write!(f, "warn"),
            Severity::Info => write!(f, "info"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EventLocation {
    pub seq: usize,
    pub line: usize, // 1-indexed line in events.ndjson
    pub event_type: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LintFinding {
    pub rule_id: String,
    pub severity: Severity,
    pub message: String,
    pub location: Option<EventLocation>,
    pub fingerprint: String,
    pub help_uri: Option<String>,
    pub tags: Vec<String>,
}

impl LintFinding {
    pub fn new(
        rule_id: impl Into<String>,
        severity: Severity,
        message: impl Into<String>,
        location: Option<EventLocation>,
        tags: Vec<String>,
    ) -> Self {
        let rule_id = rule_id.into();
        let message = message.into();

        // Compute stable fingerprint: sha256(rule_id + location_key)
        let location_key = match &location {
            Some(loc) => format!("{}:{}", loc.seq, loc.line),
            None => "global".into(),
        };
        let fingerprint = format!(
            "sha256:{}",
            hex::encode(Sha256::digest(
                format!("{}:{}", rule_id, location_key).as_bytes()
            ))
        );

        Self {
            rule_id,
            severity,
            message,
            location,
            fingerprint,
            help_uri: None,
            tags,
        }
    }

    pub fn with_help_uri(mut self, uri: impl Into<String>) -> Self {
        self.help_uri = Some(uri.into());
        self
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LintSummary {
    pub total: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct LintReport {
    pub tool_version: String,
    pub bundle_meta: Manifest,
    pub verified: bool,
    pub findings: Vec<LintFinding>,
    pub summary: LintSummary,
}

impl LintReport {
    pub fn has_findings_at_or_above(&self, threshold: &Severity) -> bool {
        self.findings.iter().any(|f| match threshold {
            Severity::Error => f.severity == Severity::Error,
            Severity::Warn => f.severity == Severity::Error || f.severity == Severity::Warn,
            Severity::Info => true,
        })
    }
}
