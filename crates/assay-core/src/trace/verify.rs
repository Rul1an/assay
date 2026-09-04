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

    if !missing.is_empty() || !truncated_shape.is_empty() {
        let total_unresolved = missing.len() + truncated_shape.len();
        let mut report = format!(
            "❌ Trace Verification Failed ({} unresolved test{}):\n",
            total_unresolved,
            if total_unresolved == 1 { "" } else { "s" }
        );

        if !missing.is_empty() {
            let count_desc = if missing.len() == 1 {
                "1 test".to_string()
            } else {
                format!("{} tests", missing.len())
            };
            report.push_str(&format!(
                "  • {} missing matching prompt in trace:\n",
                count_desc
            ));
            for id in &missing {
                report.push_str(&format!("     - {}\n", id));
            }
        }

        if !truncated_shape.is_empty() {
            let count_desc = if truncated_shape.len() == 1 {
                "1 test".to_string()
            } else {
                format!("{} tests", truncated_shape.len())
            };
            report.push_str(&format!(
                "  • {} matches stage-local truncation shape (exact prompt coverage cannot be established):\n",
                count_desc
            ));
            for id in &truncated_shape {
                report.push_str(&format!("     - {}\n", id));
            }
        }

        eprint!("{}", report);
        anyhow::bail!("{}", report.trim_end());
    }

    println!(
        "✅ Trace Verification Passed: All {} config tests found in trace.",
        cfg.tests.len()
    );
    Ok(())
}
