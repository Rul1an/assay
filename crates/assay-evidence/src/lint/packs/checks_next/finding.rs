use super::super::CheckContext;
use crate::lint::packs::schema::{PackRule, Severity};
use crate::lint::{EventLocation, LintFinding};
use crate::types::EvidenceEvent;
use sha2::Digest;

/// Create a lint finding for a pack rule.
pub(in crate::lint::packs::checks) fn create_finding(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    message: String,
    location: Option<EventLocation>,
) -> LintFinding {
    create_finding_with_severity(rule, ctx, message, location, rule.severity)
}

/// Create a lint finding with explicit severity.
pub(super) fn create_finding_with_severity(
    rule: &PackRule,
    ctx: &CheckContext<'_>,
    message: String,
    location: Option<EventLocation>,
    severity: Severity,
) -> LintFinding {
    let canonical_id = format!("{}@{}:{}", ctx.pack_name, ctx.pack_version, rule.id);

    let location_key = match &location {
        Some(loc) => format!("{}:{}", loc.seq, loc.line),
        None => "global".into(),
    };

    let fingerprint = format!(
        "sha256:{}",
        hex::encode(sha2::Sha256::digest(
            format!("{}:{}:{}", canonical_id, location_key, ctx.pack_digest).as_bytes()
        ))
    );

    let start_line = location.as_ref().map(|l| l.line).unwrap_or(1);
    let artifact_uri = location
        .as_ref()
        .map(|_| "events.ndjson")
        .unwrap_or(ctx.bundle_path);

    let primary_hash = hex::encode(sha2::Sha256::digest(
        format!(
            "{}:{}:{}:{}",
            canonical_id, artifact_uri, start_line, ctx.pack_digest
        )
        .as_bytes(),
    ));

    LintFinding {
        rule_id: canonical_id,
        severity,
        message,
        location,
        fingerprint,
        help_uri: None,
        tags: vec![ctx.pack_name.to_string(), format!("pack:{}", ctx.pack_name)],
    }
    .with_pack_metadata(
        ctx.pack_name,
        ctx.pack_version,
        &rule.id,
        rule.article_ref.as_deref(),
        &primary_hash,
    )
}

pub(super) fn event_location(event: &EvidenceEvent) -> EventLocation {
    EventLocation {
        seq: event.seq as usize,
        line: event.seq as usize + 1,
        event_type: Some(event.type_.clone()),
    }
}

trait LintFindingExt {
    fn with_pack_metadata(
        self,
        pack_name: &str,
        pack_version: &str,
        short_id: &str,
        article_ref: Option<&str>,
        primary_hash: &str,
    ) -> Self;
}

impl LintFindingExt for LintFinding {
    fn with_pack_metadata(
        mut self,
        pack_name: &str,
        pack_version: &str,
        short_id: &str,
        article_ref: Option<&str>,
        primary_hash: &str,
    ) -> Self {
        self.tags.push(format!("pack_version:{}", pack_version));
        self.tags.push(format!("short_id:{}", short_id));
        if let Some(ref_) = article_ref {
            self.tags.push(format!("article_ref:{}", ref_));
        }
        self.tags
            .push(format!("primaryLocationLineHash:{}", primary_hash));
        let _ = pack_name;
        self
    }
}
