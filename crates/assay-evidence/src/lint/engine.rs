use super::packs::executor::{PackExecutionMeta, PackExecutor, PackInfo};
use super::packs::LoadedPack;
use super::rules::{LintContext, RULES};
use super::{LintFinding, LintReport, LintSummary, Severity};
use crate::bundle::writer::{verify_bundle_with_limits, VerifyLimits};
use crate::ndjson::NdjsonEvents;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use std::io::{BufReader, Cursor, Read};

/// Lint options for bundle linting.
#[derive(Debug, Clone, Default)]
pub struct LintOptions {
    /// Loaded packs to run (in addition to built-in rules).
    pub packs: Vec<LoadedPack>,
    /// Maximum results (for GitHub SARIF limits).
    pub max_results: Option<usize>,
    /// Bundle path (for SARIF locations).
    pub bundle_path: Option<String>,
}

/// Extended lint report with pack metadata.
#[derive(Debug, Clone)]
pub struct LintReportWithPacks {
    /// Base lint report.
    pub report: LintReport,
    /// Pack execution metadata.
    pub pack_meta: Option<PackExecutionMeta>,
}

/// Lint a bundle: verify first, then apply lint rules to each event.
///
/// Returns a `LintReport` with findings. Hard-fails if verification fails.
pub fn lint_bundle<R: Read>(reader: R, limits: VerifyLimits) -> Result<LintReport> {
    lint_bundle_with_options(reader, limits, LintOptions::default()).map(|r| r.report)
}

/// Lint a bundle with options (including packs).
pub fn lint_bundle_with_options<R: Read>(
    reader: R,
    limits: VerifyLimits,
    options: LintOptions,
) -> Result<LintReportWithPacks> {
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

    // 2. Extract events
    let events_bytes = extract_events_bytes(&data)?;
    let events = parse_events(&events_bytes)?;

    // 3. Run built-in lint rules
    let mut findings = run_builtin_rules(&events);

    // 4. Run pack rules (if any)
    let pack_meta = if !options.packs.is_empty() {
        let bundle_path = options.bundle_path.as_deref().unwrap_or("bundle.tar.gz");
        let executor = PackExecutor::new(options.packs.clone())
            .map_err(|e| anyhow::anyhow!("Pack loading failed: {}", e))?;

        let max_results = options.max_results.unwrap_or(500);
        let (pack_findings, truncated, truncated_count) =
            executor.execute_with_limit(&events, &manifest, bundle_path, max_results);

        findings.extend(pack_findings);

        Some(PackExecutionMeta {
            packs: executor.packs().iter().map(PackInfo::from).collect(),
            disclaimer: executor.combined_disclaimer(),
            truncated,
            truncated_count,
        })
    } else {
        None
    };

    let summary = compute_summary(&findings);

    Ok(LintReportWithPacks {
        report: LintReport {
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            bundle_meta: manifest,
            verified: true,
            findings,
            summary,
        },
        pack_meta,
    })
}

/// Parse all events from NDJSON bytes.
fn parse_events(events_bytes: &[u8]) -> Result<Vec<EvidenceEvent>> {
    let reader = BufReader::new(Cursor::new(events_bytes));
    let events_iter = NdjsonEvents::new(reader);

    let mut events = Vec::new();
    for event_result in events_iter {
        events.push(event_result.context("parsing event")?);
    }
    Ok(events)
}

/// Run built-in lint rules on events.
fn run_builtin_rules(events: &[EvidenceEvent]) -> Vec<LintFinding> {
    let mut findings = Vec::new();

    for (line_idx, event) in events.iter().enumerate() {
        let ctx = LintContext {
            line_number: line_idx + 1,
            seq: event.seq as usize,
        };

        for rule in RULES {
            if let Some(finding) = (rule.check)(event, &ctx) {
                findings.push(finding);
            }
        }
    }

    findings
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
