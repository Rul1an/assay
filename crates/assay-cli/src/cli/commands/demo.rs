use crate::cli::args::DemoArgs;
use assay_core::config::path_resolver::PathResolver;
use assay_core::validate::{validate, ValidateOptions};
use std::fs;

pub async fn cmd_demo(args: DemoArgs) -> anyhow::Result<i32> {
    let demo_dir = args.out;
    fs::create_dir_all(&demo_dir)?;

    // 1. Create Policy File (The Rules)
    let policy_path = demo_dir.join("policy.yaml");
    let policy_content = r#"version: 1
name: demo-policy
tools:
  Search:
    args:
      properties:
        query: { pattern: "^[a-zA-Z0-9 ]+$" }
  Calculate:
    args:
      properties:
        operation: { enum: ["add", "subtract"] }
"#;
    let _ = fs::write(&policy_path, policy_content);

    // 2. Create Config File (The Test Runner)
    let config_path = demo_dir.join("assay.yaml");
    let config_content = r#"version: 1
suite: demo
model: gpt-4o-mini
tests:
  - id: demo_trace_1
    input: "find assay rules"
    expected:
      type: args_valid
      policy: policy.yaml
"#;
    let _ = fs::write(&config_path, config_content);

    // 2. Create Traces
    let trace_path = demo_dir.join("traces.jsonl");
    // We add an 'id' or 'prompt' to match the test case
    let trace_content = r#"{"id": "demo_trace_1", "tool": "Search", "args": {"query": "assay rules"}, "prompt": "find assay rules", "response": "detecting 123"}
{"tool": "Calculate", "args": {"operation": "add", "x": 1, "y": 2}, "response": "3"}
"#;
    let _ = fs::write(&trace_path, trace_content);

    println!("✓ Created demo environment in {}", demo_dir.display());
    println!("  - Config: {}", config_path.display());
    println!("  - Policy: {}", policy_path.display());
    println!("  - Traces: {}", trace_path.display());
    println!();
    println!("Running validation...");
    println!();

    // 3. Run Validation
    let cfg = assay_core::config::load_config(&config_path, false, true)?;
    let resolver = PathResolver::new(&config_path);

    let opts = ValidateOptions {
        trace_file: Some(trace_path.clone()),
        baseline_file: None,
        replay_strict: false,
    };

    let report = validate(&cfg, &opts, &resolver).await?;

    // Print success
    if report.diagnostics.is_empty() {
        println!("✅ Validation Passed!");
        println!();
        println!("Next steps:");
        println!("  1. Edit the policy: vim {}", policy_path.display());
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
