use super::field_readings::{write_event_readings, write_incomplete};
use super::observation::ObservedTraceEvent;
use super::upgrader::StreamUpgrader;
use crate::model::EvalConfig;
use anyhow::Context;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

/// Prompt coverage only. Output and verdict are unchanged from before #2782; the
/// observation carrier is consumed by [`verify_coverage_observed`].
pub fn verify_coverage(trace_path: &Path, cfg: &EvalConfig) -> anyhow::Result<()> {
    let reader = open(trace_path)?;
    let prompts = collect_prompts(reader, |_, _| Ok(())).map_err(|f| f.source)?;
    let covered = evaluate_coverage(&prompts, cfg).map_err(|report| anyhow::anyhow!(report))?;
    println!("{}", passed_line(covered));
    Ok(())
}

/// Same coverage verdict as [`verify_coverage`], preceded on `out` by one
/// truncation reading per yielded event and declared field root.
///
/// `trusted_stages` is the complete list of stage names whose clean observation
/// may read `measured_clean`; empty means no stage is trusted. Reported loss is
/// `lossy` regardless of trust. Rows are streamed as events are read; when reading
/// fails after some rows, an `incomplete` line is written and the error returned.
pub fn verify_coverage_observed(
    trace_path: &Path,
    cfg: &EvalConfig,
    trusted_stages: &[&str],
    out: &mut dyn Write,
) -> anyhow::Result<()> {
    let reader = open(trace_path)?;
    let prompts = collect_prompts(reader, |ordinal, observed| {
        write_event_readings(out, ordinal, observed, trusted_stages)
            .context("failed to write truncation reading")
    })
    .map_err(|failure| {
        // Best effort: the stream error is what the caller must see, and if `out`
        // is the reason we are here this write fails the same way.
        let _ = write_incomplete(out, failure.after_ordinal, &format!("{:#}", failure.source));
        failure.source
    })?;
    let covered = evaluate_coverage(&prompts, cfg).map_err(|report| anyhow::anyhow!(report))?;
    writeln!(out, "{}", passed_line(covered)).context("failed to write verification result")?;
    Ok(())
}

struct StreamFailure {
    after_ordinal: u64,
    source: anyhow::Error,
}

fn open(trace_path: &Path) -> anyhow::Result<BufReader<File>> {
    let file = File::open(trace_path).context("failed to open trace file")?;
    Ok(BufReader::new(file))
}

/// One pass over the observed stream. `on_event` sees every successfully yielded
/// event with its ordinal (1-based, advancing for every event kind); the returned
/// set holds the EpisodeStart prompts used for coverage membership.
fn collect_prompts(
    reader: BufReader<File>,
    mut on_event: impl FnMut(u64, &ObservedTraceEvent) -> anyhow::Result<()>,
) -> Result<HashSet<String>, StreamFailure> {
    let mut prompts = HashSet::new();
    let mut ordinal: u64 = 0;
    for event_result in StreamUpgrader::new(reader).observed() {
        let observed = event_result
            .context("failed to parse trace entry")
            .map_err(|source| StreamFailure {
                after_ordinal: ordinal,
                source,
            })?;
        ordinal += 1;
        on_event(ordinal, &observed).map_err(|source| StreamFailure {
            after_ordinal: ordinal,
            source,
        })?;
        if let super::schema::TraceEvent::EpisodeStart(start) = observed.event() {
            if let Some(prompt) = start.input.get("prompt").and_then(|v| v.as_str()) {
                prompts.insert(prompt.to_string());
            }
        }
    }
    Ok(prompts)
}

fn passed_line(covered: usize) -> String {
    format!("✅ Trace Verification Passed: All {covered} config tests found in trace.")
}

/// `Ok(count)` when every configured prompt is present verbatim; otherwise the
/// failure report. Exact membership and stage-local truncation shape are kept
/// apart, and neither is an observation reading.
fn evaluate_coverage(trace_prompts: &HashSet<String>, cfg: &EvalConfig) -> Result<usize, String> {
    let mut missing: Vec<String> = Vec::new();
    let mut truncated_shape: Vec<String> = Vec::new();

    for tc in &cfg.tests {
        if trace_prompts.contains(&tc.input.prompt) {
            continue;
        }

        let mut expected_truncated = tc.input.prompt.clone();
        if super::truncation::truncate_string(&mut expected_truncated, "prompt").is_some()
            && trace_prompts.contains(&expected_truncated)
        {
            truncated_shape.push(tc.id.clone());
        } else {
            missing.push(tc.id.clone());
        }
    }

    if missing.is_empty() && truncated_shape.is_empty() {
        return Ok(cfg.tests.len());
    }

    let total_unresolved = missing.len() + truncated_shape.len();
    let mut report = format!(
        "❌ Trace Verification Failed ({} unresolved test{}):\n",
        total_unresolved,
        if total_unresolved == 1 { "" } else { "s" }
    );

    if !missing.is_empty() {
        report.push_str(&format!(
            "  • {} missing matching prompt in trace:\n",
            count_desc(missing.len())
        ));
        for id in &missing {
            report.push_str(&format!("     - {}\n", id));
        }
    }

    if !truncated_shape.is_empty() {
        let verb = if truncated_shape.len() == 1 {
            "matches"
        } else {
            "match"
        };
        report.push_str(&format!(
            "  • {} {} stage-local truncation shape (exact prompt coverage cannot be established):\n",
            count_desc(truncated_shape.len()),
            verb
        ));
        for id in &truncated_shape {
            report.push_str(&format!("     - {}\n", id));
        }
    }

    Err(report.trim_end().to_string())
}

fn count_desc(n: usize) -> String {
    if n == 1 {
        "1 test".to_string()
    } else {
        format!("{n} tests")
    }
}
