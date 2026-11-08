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
    // We add an 'id' or 'prompt' to match the test case
    let trace_content = r#"{"id": "demo_trace_1", "tool": "Search", "args": {"query": "assay rules"}, "prompt": "find assay rules", "response": "detecting 123"}
{"id": "demo_trace_2", "tool": "Calculate", "args": {"operation": "add", "x": 1, "y": 2}, "prompt": "calculate 1 + 2", "response": "3"}
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
