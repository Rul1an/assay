//! Human rendering for `assay evidence diff`, on stderr.
//!
//! Every string that comes out of a bundle passes through `sanitize_terminal` before it reaches
//! the terminal; content ids are printed as verified (they are `sha256:` hex by construction).
//! The closing line is one of two fixed sentences pinned by
//! `tests/evidence_diff_stdout_contract.rs`, and the differs sentence is emitted whenever the
//! verified `run_root`s differ, whatever the subject projections say (#3037).

use assay_evidence::diff::{BundleComparison, DiffSet, EventDiff, EventRef};
use assay_evidence::sanitize::sanitize_terminal;

const COMPARISON_SCOPE_SENTENCE: &str =
    "Comparison scope: retained verified events. Absence and completeness are not established.";

/// Content ids listed per side in the human report; the JSON carries the full lists.
const EVENT_LIST_CAP: usize = 20;

pub(super) fn render_human(comparison: &BundleComparison) {
    let report = &comparison.report;
    eprintln!("Assay Evidence Diff");
    eprintln!("===================");
    eprintln!(
        "Baseline:  {} ({} events)",
        sanitize_terminal(&report.baseline.run_id),
        report.baseline.event_count
    );
    eprintln!(
        "  run_root {}",
        sanitize_terminal(&report.baseline.run_root)
    );
    eprintln!(
        "Candidate: {} ({} events)",
        sanitize_terminal(&report.candidate.run_id),
        report.candidate.event_count
    );
    eprintln!(
        "  run_root {}",
        sanitize_terminal(&report.candidate.run_root)
    );
    eprintln!("Event count delta: {:+}", report.summary.event_count_delta);
    eprintln!();
    eprintln!("{COMPARISON_SCOPE_SENTENCE}");
    eprintln!();

    print_diff_set("Network", &report.network);
    print_diff_set("Filesystem", &report.filesystem);
    print_diff_set("Processes", &report.processes);
    print_event_diff(&comparison.retained_events);

    let events = &comparison.retained_events;
    if events.is_empty() {
        eprintln!("No differences in retained verified events: run_root equal.");
    } else if events.added.is_empty() && events.removed.is_empty() {
        eprintln!(
            "Retained verified events differ: run_root differs; no content id added or removed \
             (order or multiplicity differs)."
        );
    } else {
        eprintln!(
            "Retained verified events differ: run_root differs; {} added, {} removed by content id.",
            events.added.len(),
            events.removed.len()
        );
    }
}

fn print_diff_set(category: &str, diff: &DiffSet) {
    if diff.is_empty() {
        return;
    }
    eprintln!("{}:", category);
    for added in &diff.added {
        eprintln!("  + {}", sanitize_terminal(added));
    }
    for removed in &diff.removed {
        eprintln!("  - {}", sanitize_terminal(removed));
    }
    eprintln!();
}

fn print_event_diff(events: &EventDiff) {
    if events.added.is_empty() && events.removed.is_empty() {
        return;
    }
    eprintln!("Retained events (by content id):");
    print_event_side('+', "candidate", &events.added);
    print_event_side('-', "baseline", &events.removed);
    eprintln!();
}

fn print_event_side(sign: char, side: &str, refs: &[EventRef]) {
    for event in refs.iter().take(EVENT_LIST_CAP) {
        eprintln!(
            "  {} {}  {}  ({} seq {})",
            sign,
            sanitize_terminal(&event.content_hash),
            sanitize_terminal(&event.type_),
            side,
            event.seq
        );
    }
    if refs.len() > EVENT_LIST_CAP {
        eprintln!(
            "  {} ... {} more (see --format json)",
            sign,
            refs.len() - EVENT_LIST_CAP
        );
    }
}
