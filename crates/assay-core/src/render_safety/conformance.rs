//! `assay.render_safety_conformance.v0` — the render-sink release-gate witness (MCP01a).
//!
//! Runs the shared corpus through the real render-safety pipeline for each sink and records, per sink:
//! hostile probes neutralised, benign controls preserved, redaction-before-truncation, terminal
//! control stripped, sink-specific encoding, and zero raw leak counts. The MCP01 Strong claim hangs
//! on THIS conformance, not on the capture receipt (which proves only that capture redaction ran).

use super::corpus::{corpus_digest, BENIGN, HOSTILE};
use super::{
    has_residual_control, render_safe, render_truncate_first_unsafe, Sink, MAX_RENDER_FIELD,
};
use serde::{Deserialize, Serialize};

pub const SCHEMA: &str = "assay.render_safety_conformance.v0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SinkConformance {
    pub sink: String,
    pub renderer: String,
    pub hostile_probe_count: usize,
    pub benign_control_count: usize,
    pub raw_secret_leak_count: usize,
    pub raw_pii_leak_count: usize,
    pub terminal_control_leak_count: usize,
    pub redaction_before_truncation: bool,
    pub benign_preserved: bool,
    pub sink_specific_encoding: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderSafetyConformance {
    pub schema: String,
    pub corpus_digest: String,
    pub sinks: Vec<SinkConformance>,
}

fn sink_conformance(sink: Sink) -> SinkConformance {
    let mut raw_secret_leak_count = 0;
    let mut raw_pii_leak_count = 0;
    let mut terminal_control_leak_count = 0;

    for probe in HOSTILE.iter() {
        let out = render_safe(sink, &probe.input, MAX_RENDER_FIELD);
        match probe.class {
            // Control: reject ANY residual terminal/bidi control, not just this probe's needle.
            "control" => {
                if has_residual_control(&out) {
                    terminal_control_leak_count += 1;
                }
            }
            "secret" if out.contains(&probe.needle) => raw_secret_leak_count += 1,
            "pii" if out.contains(&probe.needle) => raw_pii_leak_count += 1,
            _ => {}
        }
    }

    // Benign near-matches must survive rendering (not over-redacted, not encoded away).
    let benign_preserved = BENIGN
        .iter()
        .all(|probe| render_safe(sink, &probe.input, MAX_RENDER_FIELD).contains(&probe.needle));

    // Differential: redact-before-truncate must not leak where truncate-first would, proving order.
    let boundary = HOSTILE
        .iter()
        .find(|x| x.name == "long_secret_prefix")
        .expect("corpus has long_secret_prefix");
    let safe = render_safe(sink, &boundary.input, MAX_RENDER_FIELD);
    let unsafe_out = render_truncate_first_unsafe(sink, &boundary.input, MAX_RENDER_FIELD);
    let redaction_before_truncation = !safe.contains("ghp_") && unsafe_out.contains("ghp_");

    SinkConformance {
        sink: sink.as_str().to_string(),
        renderer: "assay-core".to_string(),
        hostile_probe_count: HOSTILE.len(),
        benign_control_count: BENIGN.len(),
        raw_secret_leak_count,
        raw_pii_leak_count,
        terminal_control_leak_count,
        redaction_before_truncation,
        benign_preserved,
        sink_specific_encoding: sink.encoding().to_string(),
    }
}

/// Run the corpus through every sink and produce the conformance report.
pub fn run_conformance() -> RenderSafetyConformance {
    RenderSafetyConformance {
        schema: SCHEMA.to_string(),
        corpus_digest: corpus_digest(),
        sinks: Sink::ALL.iter().map(|s| sink_conformance(*s)).collect(),
    }
}

/// A conformance report is clean only when every sink leaks nothing, preserves benign output, and
/// honours redaction-before-truncation.
pub fn is_clean(report: &RenderSafetyConformance) -> bool {
    report.schema == SCHEMA
        && !report.sinks.is_empty()
        && report.sinks.iter().all(|s| {
            s.raw_secret_leak_count == 0
                && s.raw_pii_leak_count == 0
                && s.terminal_control_leak_count == 0
                && s.benign_preserved
                && s.redaction_before_truncation
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conformance_is_clean_across_all_sinks() {
        let report = run_conformance();
        assert_eq!(report.sinks.len(), Sink::ALL.len());
        assert!(
            is_clean(&report),
            "render-safety conformance not clean: {report:?}"
        );
    }
}
