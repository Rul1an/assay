use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

use assay_core::mcp::{mcp_events_to_v2_trace, parse_mcp_transcript, McpInputFormat};
use assay_core::trace::schema::TraceEvent;

#[derive(Debug, Clone)]
pub struct ImportMcpArgs {
    pub input: PathBuf,
    pub out_trace: PathBuf,
    pub format: McpInputFormat,
    pub episode_id: Option<String>,
    pub test_id: Option<String>,
    pub prompt: Option<String>,
}

pub fn run(args: ImportMcpArgs) -> Result<()> {
    let text = fs::read_to_string(&args.input)
        .with_context(|| format!("failed to read input: {:?}", args.input))?;

    println!("Reading MCP transcript from: {:?}", args.input);
    let events =
        parse_mcp_transcript(&text, args.format).context("failed to parse MCP transcript")?;
    println!("Parsed {} MCP events.", events.len());

    let episode_id = args.episode_id.clone().unwrap_or_else(|| {
        // Fallback to filename stem
        args.input
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "mcp_episode".to_string())
    });

    let trace: Vec<TraceEvent> =
        mcp_events_to_v2_trace(events, episode_id, args.test_id, args.prompt);
    println!("Generated {} Assay V2 trace events.", trace.len());

    let mut buf = String::new();
    for ev in trace {
        buf.push_str(&serde_json::to_string(&ev)?);
        buf.push('\n');
    }

    fs::write(&args.out_trace, buf)
        .with_context(|| format!("failed to write out-trace: {:?}", args.out_trace))?;

    println!("Trace written to: {:?}", args.out_trace);

    Ok(())
}
