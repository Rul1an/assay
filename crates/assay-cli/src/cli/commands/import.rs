use super::exit_codes;
use crate::cli::args::ImportArgs;
use anyhow::{Context, Result};
use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat};
use assay_core::trace::schema::TraceEvent;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub fn cmd_import(args: ImportArgs) -> Result<i32> {
    let input_path = args.input;
    let format_enum = match args.format.as_str() {
        "inspector" | "mcp-inspector" | "mcp-inspector@v1" => McpInputFormat::Inspector,
        "jsonrpc" => McpInputFormat::JsonRpc,
        other => anyhow::bail!("unknown format: {}", other),
    };

    println!("Importing MCP transcript from: {:?}", input_path);
    let text = fs::read_to_string(&input_path)
        .with_context(|| format!("failed to read input: {:?}", input_path))?;

    let events =
        parse_mcp_transcript(&text, format_enum).context("failed to parse MCP transcript")?;
    println!("Parsed {} MCP events.", events.len());

    // Derive output path if needed
    let out_path = args.out_trace.unwrap_or_else(|| {
        let mut p = input_path.clone();
        if let Some(stem) = p.file_stem() {
            let s = stem.to_string_lossy();
            p.set_file_name(format!("{}.trace.jsonl", s));
        } else {
            p.set_file_name("trace.jsonl");
        }
        p
    });

    let episode_id = input_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mcp_import".to_string());

    let trace: Vec<TraceEvent> = mcp_events_to_v2_trace(events, episode_id, None, None);
    println!("Generated {} Assay V2 trace events.", trace.len());

    let mut buf = String::new();
    let mut tools_found = BTreeSet::new();
    let mut actual_sequence = Vec::new();

    for ev in trace {
        if let TraceEvent::ToolCall(tc) = &ev {
            tools_found.insert(tc.tool_name.clone());
            actual_sequence.push(tc.tool_name.clone());
        }
        buf.push_str(&serde_json::to_string(&ev)?);
        buf.push('\n');
    }

    fs::write(&out_path, buf)
        .with_context(|| format!("failed to write out-trace: {:?}", out_path))?;
    println!("✅ Trace written to: {:?}", out_path);

    if args.init {
        init_scaffolding(&tools_found, &actual_sequence, &out_path)?;
    }

    Ok(exit_codes::OK)
}

fn init_scaffolding(
    tools: &BTreeSet<String>,
    _sequence: &[String],
    trace_path: &Path,
) -> Result<()> {
    println!("\nInitializing MCP Evaluation Scaffolding... 🏗️");

    // 1. Create mcp-eval.yaml
    let config_path = Path::new("mcp-eval.yaml");
    if config_path.exists() {
        println!("⚠️  {} already exists, skipping.", config_path.display());
    } else {
        let mut yaml = String::new();
        yaml.push_str("# Auto-generated MCP Evaluation Config\n");
        yaml.push_str("configVersion: 1\n");
        yaml.push_str("suite: mcp-suite\n");
        yaml.push_str("model: mcp-replay\n");
        yaml.push_str("settings:\n");
        yaml.push_str("  parallel: 1\n");
        yaml.push_str("tests:\n");

        if tools.is_empty() {
            yaml.push_str("# No tools found in trace.\n");
        } else {
            yaml.push_str("  - id: mcp-quality-gate\n");
            yaml.push_str("    input:\n");
            yaml.push_str("      prompt: \"<mcp:session>\"\n");
            yaml.push_str("    expected:\n");
            yaml.push_str("      type: args_valid\n");
            yaml.push_str("      # Inline schema validation (default: strict object)\n");
            yaml.push_str("      schema:\n");
            for tool in tools {
                yaml.push_str(&format!("        {}:\n", tool));
                yaml.push_str("          type: object\n");
                yaml.push_str("          additionalProperties: true\n");
                yaml.push_str("          properties: {}\n");
            }
            yaml.push('\n');
            yaml.push_str("  - id: mcp-sequence-gate\n");
            yaml.push_str("    input:\n");
            yaml.push_str("      prompt: \"<mcp:session>\"\n");
            yaml.push_str("    expected:\n");
            yaml.push_str("      type: sequence_valid\n");
            yaml.push_str("      # Inline expected sequence (rules)\n");
            yaml.push_str("      rules:\n");
            // Use rules: [{type: require, tool: "foo"}]
            // We use the deduplicated tools set to avoid strict ordering brittleness by default.
            for tool_name in tools {
                yaml.push_str("        - type: require\n");
                yaml.push_str(&format!("          tool: \"{}\"\n", tool_name));
            }
        }

        fs::write(config_path, yaml)?;
        println!("✅ Created {}", config_path.display());
    }

    // Skipped: policies dir creation (using inline config)

    println!("\n🚀 Ready! Try running:");

    let _trace_id = trace_path
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .replace(".trace", "");

    println!(
        "  assay run --config mcp-eval.yaml --trace-file {} --replay-strict",
        trace_path.display()
    );
    println!(
        "\nNOTE: Ensure the 'id' in mcp-eval.yaml matches the episode ID in your trace ('...')"
    );

    Ok(())
}
