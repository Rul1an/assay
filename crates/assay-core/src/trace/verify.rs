use crate::model::EvalConfig;
use anyhow::Context;
use std::collections::HashSet;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

pub fn verify_coverage(trace_path: &Path, cfg: &EvalConfig) -> anyhow::Result<()> {
    let file = File::open(trace_path).context("failed to open trace file")?;
    let reader = BufReader::new(file);

    let mut trace_prompts = HashSet::new();
    let mut trace_ids = HashSet::new();

    let upgrader = super::upgrader::StreamUpgrader::new(reader);

    for event_result in upgrader {
        let event = event_result.context("failed to parse trace entry")?;

        // We only care about EpisodeStart to verify prompt coverage
        if let super::schema::TraceEvent::EpisodeStart(start) = event {
            // Extract prompt from input
            if let Some(prompt) = start.input.get("prompt").and_then(|v| v.as_str()) {
                trace_prompts.insert(prompt.to_string());
            }
            trace_ids.insert(start.episode_id);
        }
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
