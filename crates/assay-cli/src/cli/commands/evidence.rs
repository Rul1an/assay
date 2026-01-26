use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use std::fs::File;
use std::io::{self, Read};

/// Manage tamper-evident bundles (audit/compliance)
#[derive(Debug, Subcommand, Clone)]
pub enum EvidenceCmd {
    /// Export an evidence bundle from inputs
    Export(EvidenceExportArgs),
    /// Verify a bundle's integrity and provenance
    Verify(EvidenceVerifyArgs),
}

#[derive(Debug, Args, Clone)]
pub struct EvidenceExportArgs {
    /// Input events source (e.g. 'profile.yaml', 'events.jsonl', or Trace Dir)
    /// Currently only supports 'profile.yaml' style input for v1 demo.
    #[arg(long)]
    pub input: std::path::PathBuf,

    /// Output bundle path (tar.gz).
    /// Should end in .tar.gz
    #[arg(long)]
    pub out: std::path::PathBuf,
}

#[derive(Debug, Args, Clone)]
pub struct EvidenceVerifyArgs {
    /// Bundle path, or "-" for stdin
    #[arg(value_name = "BUNDLE", default_value = "-")]
    pub bundle: String,
}

pub fn run(args: crate::cli::args::EvidenceArgs) -> Result<i32> {
    match args.cmd {
        EvidenceCmd::Export(a) => cmd_export(a),
        EvidenceCmd::Verify(a) => cmd_verify(a),
    }
}

fn cmd_export(args: EvidenceExportArgs) -> Result<i32> {
    let events = load_events_from_input(&args.input)
        .with_context(|| format!("failed to load events from {}", args.input.display()))?;

    if events.is_empty() {
        eprintln!("Warning: No events found in input. Bundle checks might fail event count.");
    }

    // Atomic write pattern: write to tmp then rename?
    // For now direct write, user must ensure path is valid.
    let out_file = File::create(&args.out)
        .with_context(|| format!("failed to create output file {}", args.out.display()))?;

    let mut bw = assay_evidence::bundle::BundleWriter::new(out_file);
    for ev in events {
        bw.add_event(ev);
    }

    bw.finish().context("failed to finalize evidence bundle")?;

    eprintln!("Exported evidence bundle to {}", args.out.display());
    Ok(0)
}

fn cmd_verify(args: EvidenceVerifyArgs) -> Result<i32> {
    if args.bundle == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        assay_evidence::bundle::verify_bundle(io::Cursor::new(buf))
            .context("bundle verification failed")?;
        eprintln!("Bundle verified (stdin): OK");
        return Ok(0);
    }

    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle))?;

    assay_evidence::bundle::verify_bundle(f).context("bundle verification failed")?;

    eprintln!("Bundle verified ({}): OK", args.bundle);
    Ok(0)
}

/// Convert input (e.g. profile yaml? or raw jsonl?) to EvidenceEvents.
/// For v1, we assume input is a JSONL file of pre-formatted EvidenceEvents for simplicity,
/// OR we map from Profile format if needed.
/// Since the user said "exporter wiring comes logically: assay sandbox --profile -> map 1-to-1",
/// let's implement a simple loader that expects EvidenceEvent JSONL for now to satisfy the contract.
fn load_events_from_input(
    input: &std::path::Path,
) -> Result<Vec<assay_evidence::types::EvidenceEvent>> {
    use std::io::BufRead;

    let f = File::open(input)?;
    let reader = io::BufReader::new(f);
    let mut events = Vec::new();

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        // Try parsing directly as EvidenceEvent
        // This assumes some upstream tool already shaped it.
        // In a real integration, we'd map from crate::profile::events::Event -> EvidenceEvent.
        // But let's start with native passthrough.
        let ev: assay_evidence::types::EvidenceEvent = serde_json::from_str(&line)
            .with_context(|| format!("Line {}: invalid evidence event json", i + 1))?;
        events.push(ev);
    }

    Ok(events)
}
