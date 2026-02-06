use super::packs::executor::{PackExecutionMeta, PackExecutor};
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
        let bundle_path = options
            .bundle_path
            .clone()
            .unwrap_or_else(|| "bundle.tar.gz".to_string());
        // manifest.bundle_id already has sha256: prefix
        let bundle_id = Some(manifest.bundle_id.clone());

        // Take ownership of packs to avoid clone
        let executor = PackExecutor::new(options.packs)
            .map_err(|e| anyhow::anyhow!("Pack loading failed: {}", e))?;

        let pack_findings = executor.execute(&events, &manifest, &bundle_path);
        findings.extend(pack_findings);

        // Build metadata using helper (includes rule_metadata and anchor_file)
        Some(executor.build_meta(
            Some(bundle_path),
            bundle_id,
            false, // Will be set below after combined truncation
            0,     // Will be set below
        ))
    } else {
        None
    };

    // 5. Apply max_results to combined findings (builtin + pack)
    let max_results = options.max_results.unwrap_or(5000);
    let (findings, truncated, truncated_count) = truncate_findings(findings, max_results);

    // Update pack_meta with truncation info
    let pack_meta = pack_meta.map(|mut meta| {
        meta.truncated = truncated;
        meta.truncated_count = truncated_count;
        meta
    });

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

/// Truncate findings to max_results, removing lowest severity first.
/// Returns (findings, truncated, truncated_count).
fn truncate_findings(
    mut findings: Vec<LintFinding>,
    max_results: usize,
) -> (Vec<LintFinding>, bool, usize) {
    if findings.len() <= max_results {
        return (findings, false, 0);
    }

    // Sort by severity priority (highest first to keep)
    findings.sort_by(|a, b| {
        let a_priority = severity_priority(&a.severity);
        let b_priority = severity_priority(&b.severity);
        b_priority.cmp(&a_priority)
    });

    // Keep the top max_results (highest severity); drop the rest
    let truncated_count = findings.len() - max_results;
    findings.truncate(max_results);

    (findings, true, truncated_count)
}

/// Get severity priority for sorting.
fn severity_priority(severity: &Severity) -> u8 {
    match severity {
        Severity::Info => 0,
        Severity::Warn => 1,
        Severity::Error => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lint::EventLocation;

    fn make_finding(severity: Severity, idx: usize) -> LintFinding {
        LintFinding::new(
            format!("TEST-{:05}", idx),
            severity,
            format!("test finding {}", idx),
            Some(EventLocation {
                seq: idx,
                line: idx + 1,
                event_type: None,
            }),
            vec![],
        )
    }

    #[test]
    fn truncate_no_op_under_limit() {
        let findings: Vec<LintFinding> =
            (0..100).map(|i| make_finding(Severity::Warn, i)).collect();
        let (result, truncated, count) = truncate_findings(findings, 5000);
        assert_eq!(result.len(), 100);
        assert!(!truncated);
        assert_eq!(count, 0);
    }

    #[test]
    fn truncate_30k_to_5k_keeps_highest_severity() {
        // Simulate 30k results: 10k errors, 10k warnings, 10k infos
        let mut findings = Vec::with_capacity(30_000);
        for i in 0..10_000 {
            findings.push(make_finding(Severity::Error, i));
        }
        for i in 10_000..20_000 {
            findings.push(make_finding(Severity::Warn, i));
        }
        for i in 20_000..30_000 {
            findings.push(make_finding(Severity::Info, i));
        }

        let (result, truncated, truncated_count) = truncate_findings(findings, 5000);

        assert_eq!(result.len(), 5000);
        assert!(truncated);
        assert_eq!(truncated_count, 25_000);

        // All 10k errors should survive (highest severity), sorted first
        let errors = result
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        assert_eq!(errors, 5000);
    }

    #[test]
    fn truncate_preserves_errors_over_infos() {
        // 3 errors + 10 infos, limit 5 -> keep all 3 errors + 2 warnings/infos
        let mut findings = Vec::new();
        for i in 0..3 {
            findings.push(make_finding(Severity::Error, i));
        }
        for i in 3..13 {
            findings.push(make_finding(Severity::Info, i));
        }

        let (result, truncated, truncated_count) = truncate_findings(findings, 5);

        assert_eq!(result.len(), 5);
        assert!(truncated);
        assert_eq!(truncated_count, 8);

        let errors = result
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        assert_eq!(errors, 3);
    }

    #[test]
    fn default_max_results_is_5000() {
        let options = LintOptions::default();
        assert_eq!(options.max_results.unwrap_or(5000), 5000);
    }
}
