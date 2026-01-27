use super::rules::{LintContext, RULES};
use super::{LintFinding, LintReport, LintSummary, Severity};
use crate::bundle::writer::{verify_bundle_with_limits, VerifyLimits};
use crate::ndjson::NdjsonEvents;
use anyhow::{Context, Result};
use std::io::{BufReader, Cursor, Read};

/// Lint a bundle: verify first, then apply lint rules to each event.
///
/// Returns a `LintReport` with findings. Hard-fails if verification fails.
pub fn lint_bundle<R: Read>(reader: R, limits: VerifyLimits) -> Result<LintReport> {
    // Read entire bundle into memory (needed for two passes)
    let mut data = Vec::new();
    let mut reader = reader;
    reader
        .read_to_end(&mut data)
        .context("reading bundle data")?;

    // 1. Verify bundle integrity (hard fail)
    let verify_result = verify_bundle_with_limits(Cursor::new(&data), limits)
        .context("bundle verification failed")?;

    let manifest = verify_result.manifest.clone();

    // 2. Extract events and lint
    let events_bytes = extract_events_bytes(&data)?;
    let reader = BufReader::new(Cursor::new(events_bytes));
    let events_iter = NdjsonEvents::new(reader);

    let mut findings = Vec::new();

    for (line_idx, event_result) in events_iter.enumerate() {
        let event = event_result.context("parsing event during lint")?;
        let ctx = LintContext {
            line_number: line_idx + 1,
            seq: event.seq as usize,
        };

        for rule in RULES {
            if let Some(finding) = (rule.check)(&event, &ctx) {
                findings.push(finding);
            }
        }
    }

    let summary = compute_summary(&findings);

    Ok(LintReport {
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        bundle_meta: manifest,
        verified: true,
        findings,
        summary,
    })
}

fn extract_events_bytes(bundle_data: &[u8]) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bundle_data));
    let mut archive = tar::Archive::new(decoder);

    for entry in archive.entries().context("reading tar entries")? {
        let mut entry = entry.context("reading tar entry")?;
        let path = entry.path()?.to_string_lossy().to_string();

        if path == "events.ndjson" {
            let mut content = Vec::new();
            entry
                .read_to_end(&mut content)
                .context("reading events.ndjson")?;
            return Ok(content);
        }
    }

    anyhow::bail!("missing events.ndjson in bundle")
}

fn compute_summary(findings: &[LintFinding]) -> LintSummary {
    let errors = findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|f| f.severity == Severity::Warn)
        .count();
    let infos = findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .count();

    LintSummary {
        total: findings.len(),
        errors,
        warnings,
        infos,
    }
}
