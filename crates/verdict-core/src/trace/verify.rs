use super::schema::TraceEntryV1;
use crate::model::EvalConfig;
use anyhow::Context;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

pub fn verify_coverage(trace_path: &Path, cfg: &EvalConfig) -> anyhow::Result<()> {
    let file = File::open(trace_path).context("failed to open trace file")?;
    let reader = BufReader::new(file);

    let mut trace_prompts = HashSet::new();
    let mut trace_ids = HashSet::new();

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: TraceEntryV1 = serde_json::from_str(&line)
            .context(format!("failed to parse trace entry at line {}", i + 1))?;

        trace_prompts.insert(entry.prompt);
        trace_ids.insert(entry.request_id);
    }

    let mut missing = Vec::new();

    for tc in &cfg.tests {
        if !trace_prompts.contains(&tc.input.prompt) {
            // Heuristic: check if ID match exists, might be a prompt mismatch warning
            missing.push(tc.id.clone());
        }
    }

    if !missing.is_empty() {
        // Pretty print missing
        eprintln!(
            "❌ Trace Verification Failed: {} tests missing matching prompt in trace.",
            missing.len()
        );
        for id in missing {
            eprintln!("   - {}", id);
        }
        anyhow::bail!("Trace coverage check failed");
    }

    println!(
        "✅ Trace Verification Passed: All {} config tests found in trace.",
        cfg.tests.len()
    );
    Ok(())
}
