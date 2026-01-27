use super::{BundleSummary, DiffReport, DiffSet, DiffSummary};
use crate::bundle::reader::BundleReader;
use crate::bundle::writer::VerifyLimits;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::io::Read;

/// Diff two verified bundles and report differences in network, filesystem, and process subjects.
///
/// Both bundles are verified first (hard fail if either fails).
pub fn diff_bundles<R1: Read, R2: Read>(
    baseline: R1,
    candidate: R2,
    limits: VerifyLimits,
) -> Result<DiffReport> {
    let baseline_reader = BundleReader::open_with_limits(baseline, limits)
        .context("failed to open baseline bundle")?;
    let candidate_reader = BundleReader::open_with_limits(candidate, limits)
        .context("failed to open candidate bundle")?;

    let baseline_events = baseline_reader
        .events_vec()
        .context("reading baseline events")?;
    let candidate_events = candidate_reader
        .events_vec()
        .context("reading candidate events")?;

    let baseline_summary = make_summary(&baseline_reader, &baseline_events);
    let candidate_summary = make_summary(&candidate_reader, &candidate_events);

    let event_count_delta =
        candidate_summary.event_count as i64 - baseline_summary.event_count as i64;

    let baseline_subjects = categorize_subjects(&baseline_events);
    let candidate_subjects = categorize_subjects(&candidate_events);

    let network = compute_diff(&baseline_subjects.network, &candidate_subjects.network);
    let filesystem = compute_diff(
        &baseline_subjects.filesystem,
        &candidate_subjects.filesystem,
    );
    let processes = compute_diff(&baseline_subjects.processes, &candidate_subjects.processes);

    Ok(DiffReport {
        baseline: baseline_summary,
        candidate: candidate_summary,
        summary: DiffSummary {
            event_count_delta,
            duration_delta: None,
        },
        network,
        filesystem,
        processes,
    })
}

fn make_summary(reader: &BundleReader, events: &[EvidenceEvent]) -> BundleSummary {
    let time_range = if events.is_empty() {
        None
    } else {
        let first = events.first().unwrap().time.to_rfc3339();
        let last = events.last().unwrap().time.to_rfc3339();
        Some((first, last))
    };

    BundleSummary {
        run_id: reader.run_id().to_string(),
        event_count: reader.event_count(),
        run_root: reader.run_root().to_string(),
        time_range,
    }
}

struct CategorizedSubjects {
    network: BTreeSet<String>,
    filesystem: BTreeSet<String>,
    processes: BTreeSet<String>,
}

fn categorize_subjects(events: &[EvidenceEvent]) -> CategorizedSubjects {
    let mut result = CategorizedSubjects {
        network: BTreeSet::new(),
        filesystem: BTreeSet::new(),
        processes: BTreeSet::new(),
    };

    for event in events {
        let subject = match &event.subject {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };

        if event.type_.contains(".net.") || event.type_.ends_with(".net") {
            result.network.insert(subject);
        } else if event.type_.contains(".fs.") || event.type_.ends_with(".fs") {
            result.filesystem.insert(subject);
        } else if event.type_.contains(".process.") || event.type_.ends_with(".process") {
            result.processes.insert(subject);
        }
    }

    result
}

fn compute_diff(baseline: &BTreeSet<String>, candidate: &BTreeSet<String>) -> DiffSet {
    let added: Vec<String> = candidate.difference(baseline).cloned().collect();
    let removed: Vec<String> = baseline.difference(candidate).cloned().collect();

    DiffSet { added, removed }
}
