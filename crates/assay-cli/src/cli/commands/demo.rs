use crate::cli::args::DemoArgs;
use anyhow::Context;
use assay_core::validate::{validate_with_trace_metrics, TraceMetrics, ValidateOptions};
use std::fs;

pub async fn cmd_demo(args: DemoArgs) -> anyhow::Result<i32> {
    let demo_dir = args.out;
    fs::create_dir_all(&demo_dir)?;

    // 1. Create Config File (The Rules and The Test Runner)
    // Each test carries its tool-argument schemas inline, the form `assay run` reads without a
    // policy file and without resolving a path, so the printed `assay validate` works from any
    // directory. A tool's entry is a JSON Schema for its arguments.
    let config_path = demo_dir.join("assay.yaml");
    let config_content = r#"version: 1
suite: demo
model: gpt-4o-mini
tests:
  - id: demo_trace_1
    input: "find assay rules"
    expected:
      type: args_valid
      schema:
        Search:
          type: object
          properties:
            query:
              type: string
              pattern: "^[a-zA-Z0-9 ]+$"
  - id: demo_trace_2
    input: "calculate 1 + 2"
    expected:
      type: args_valid
      schema:
        Calculate:
          type: object
          properties:
            operation:
              type: string
              enum: ["add", "subtract"]
"#;
    fs::write(&config_path, config_content)
        .with_context(|| format!("failed to write demo file {}", config_path.display()))?;

    // 2. Create Traces
    let trace_path = demo_dir.join("traces.jsonl");
    // Preserve the tool-call metadata that replay produced for both original demo rows.
    let trace_content = r#"{"type": "episode_start", "episode_id": "demo_trace_1", "timestamp": 0, "input": {"prompt": "find assay rules"}, "meta": {"tool_calls": [{"id": "legacy-v1", "tool_name": "Search", "args": {"query": "assay rules"}, "result": null, "error": null, "index": 0, "ts_ms": 0}]}}
{"type": "step", "episode_id": "demo_trace_1", "step_id": "demo_trace_1-step-0", "timestamp": 1, "kind": "llm_completion", "content": "detecting 123"}
{"type": "episode_end", "episode_id": "demo_trace_1", "timestamp": 2, "final_output": "detecting 123"}
{"type": "episode_start", "episode_id": "demo_trace_2", "timestamp": 0, "input": {"prompt": "calculate 1 + 2"}, "meta": {"tool_calls": [{"id": "legacy-v1", "tool_name": "Calculate", "args": {"operation": "add", "x": 1, "y": 2}, "result": null, "error": null, "index": 0, "ts_ms": 0}]}}
{"type": "step", "episode_id": "demo_trace_2", "step_id": "demo_trace_2-step-0", "timestamp": 1, "kind": "llm_completion", "content": "3"}
{"type": "episode_end", "episode_id": "demo_trace_2", "timestamp": 2, "final_output": "3"}
"#;
    fs::write(&trace_path, trace_content)
        .with_context(|| format!("failed to write demo file {}", trace_path.display()))?;

    println!("✓ Created demo environment in {}", demo_dir.display());
    println!("  - Config: {}", config_path.display());
    println!("  - Traces: {}", trace_path.display());
    println!();
    println!("Running validation...");
    println!();

    // 3. Run Validation, deciding each test with the metric `assay run` uses, as
    // `assay validate` does.
    let cfg = assay_core::config::load_config(&config_path, false, true)?;

    let opts = ValidateOptions {
        trace_file: Some(trace_path.clone()),
        baseline_file: None,
        replay_strict: false,
    };
    let metrics = TraceMetrics {
        sequence_valid: &assay_metrics::sequence_valid::SequenceValidMetric,
        args_valid: &assay_metrics::args_valid::ArgsValidMetric,
    };

    let report = validate_with_trace_metrics(&cfg, &opts, &metrics).await?;

    // Print success
    if report.diagnostics.is_empty() {
        println!("✅ Validation Passed!");
        println!("  Rules checked:");
        println!("    - Search.query");
        println!("    - Calculate.operation");
        println!();
        println!("Next steps:");
        println!("  1. Edit the rules:  vim {}", config_path.display());
        println!(
            "  2. Run validation:  assay validate --config {} --trace-file {}",
            config_path.display(),
            trace_path.display()
        );
        Ok(0)
    } else {
        // Should not happen in demo, but just in case
        for d in report.diagnostics {
            println!("{}", d.format_terminal());
        }
        Ok(1)
    }
}
