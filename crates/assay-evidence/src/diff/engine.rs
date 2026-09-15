use super::{
    BundleComparison, BundleSummary, DiffReport, DiffSet, DiffSummary, EventDiff, EventRef,
};
use crate::bundle::reader::BundleReader;
use crate::bundle::writer::VerifyLimits;
use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;

/// Diff two verified bundles and report differences in network, filesystem, and process subjects.
///
/// The projection layer of [`compare_bundles`]; the retained events outside those projections are
/// not in this report. Both bundles are verified first (hard fail if either fails).
pub fn diff_bundles<R1: Read, R2: Read>(
    baseline: R1,
    candidate: R2,
    limits: VerifyLimits,
) -> Result<DiffReport> {
    compare_bundles(baseline, candidate, limits).map(|comparison| comparison.report)
}

/// Compare two verified bundles on both layers: the retained events by verified content id (with
/// `run_root` as the whole-record witness), and the network, filesystem and process subject
/// projections.
///
/// Both bundles are verified first (hard fail if either fails).
pub fn compare_bundles<R1: Read, R2: Read>(
    baseline: R1,
    candidate: R2,
    limits: VerifyLimits,
) -> Result<BundleComparison> {
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

    let retained_events = compare_events(
        &baseline_events,
        &candidate_events,
        baseline_summary.run_root == candidate_summary.run_root,
    )?;

    Ok(BundleComparison {
        report: DiffReport {
            baseline: baseline_summary,
            candidate: candidate_summary,
            summary: DiffSummary {
                event_count_delta,
                duration_delta: None,
            },
            network,
            filesystem,
            processes,
        },
        retained_events,
    })
}

/// Compare the retained events by content id. The ids were recomputed by verification, so they
/// are the same function `run_root` is built from; this is the record-level reading of that root,
/// not a second identity.
fn compare_events(
    baseline: &[EvidenceEvent],
    candidate: &[EvidenceEvent],
    run_root_equal: bool,
) -> Result<EventDiff> {
    let baseline_ids = index_by_content_hash(baseline)?;
    let candidate_ids = index_by_content_hash(candidate)?;
    Ok(EventDiff {
        run_root_equal,
        added: only_in(&candidate_ids, &baseline_ids),
        removed: only_in(&baseline_ids, &candidate_ids),
    })
}

/// First occurrence per content id. An event without a content id after verification is a
/// contract breach, not an event to skip: skipping would let it vanish from the comparison.
fn index_by_content_hash(events: &[EvidenceEvent]) -> Result<BTreeMap<String, EventRef>> {
    let mut index = BTreeMap::new();
    for event in events {
        let content_hash = event
            .content_hash
            .clone()
            .with_context(|| format!("verified event seq {} has no content_hash", event.seq))?;
        index.entry(content_hash.clone()).or_insert(EventRef {
            content_hash,
            type_: event.type_.clone(),
            seq: event.seq,
        });
    }
    Ok(index)
}

fn only_in(
    these: &BTreeMap<String, EventRef>,
    others: &BTreeMap<String, EventRef>,
) -> Vec<EventRef> {
    let mut refs: Vec<EventRef> = these
        .iter()
        .filter(|(hash, _)| !others.contains_key(*hash))
        .map(|(_, event)| event.clone())
        .collect();
    refs.sort_by_key(|event| event.seq);
    refs
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
