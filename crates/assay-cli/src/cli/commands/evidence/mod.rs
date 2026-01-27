pub mod mapping;

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use mapping::{DetailLevel, EvidenceMapper};
use std::fs::File;
use std::io::{self, Read};

/// Manage tamper-evident bundles (audit/compliance)
#[derive(Debug, Subcommand, Clone)]
pub enum EvidenceCmd {
    /// Export an evidence bundle from a Profile
    Export(EvidenceExportArgs),
    /// Verify a bundle's integrity and provenance
    Verify(EvidenceVerifyArgs),
    /// Inspect a bundle's contents (verify + show table)
    Show(EvidenceShowArgs),
}

#[derive(Debug, Args, Clone)]
pub struct EvidenceExportArgs {
    /// Input Profile trace (YAML/JSON)
    #[arg(long, alias = "input")]
    pub profile: std::path::PathBuf,

    /// Output bundle path (.tar.gz). Defaults to assay_evidence_{run_id}.tar.gz
    #[arg(long, short = 'o')]
    pub out: Option<std::path::PathBuf>,

    /// Level of detail to include (summary, observed, full)
    #[arg(long, value_enum, default_value_t = DetailLevel::Observed)]
    pub detail: DetailLevel,
}

#[derive(Debug, Args, Clone)]
pub struct EvidenceVerifyArgs {
    /// Bundle path, or "-" for stdin
    #[arg(value_name = "BUNDLE", default_value = "-")]
    pub bundle: std::path::PathBuf,
}

#[derive(Debug, Args, Clone)]
pub struct EvidenceShowArgs {
    /// Bundle path
    #[arg(value_name = "BUNDLE")]
    pub bundle: std::path::PathBuf,

    /// Skip verification (show even if corrupt/untrusted)
    #[arg(long)]
    pub no_verify: bool,

    /// Output format: 'table' or 'json' (raw dump)
    #[arg(long, default_value = "table")]
    pub format: String,
}

pub fn run(args: crate::cli::args::EvidenceArgs) -> Result<i32> {
    match args.cmd {
        EvidenceCmd::Export(a) => cmd_export(a),
        EvidenceCmd::Verify(a) => cmd_verify(a),
        EvidenceCmd::Show(a) => cmd_show(a),
    }
}

fn cmd_export(args: EvidenceExportArgs) -> Result<i32> {
    // 1. Load Profile
    let profile = crate::cli::commands::profile_types::load_profile(&args.profile)
        .with_context(|| format!("failed to load profile from {}", args.profile.display()))?;

    // 2. Map Profile -> EvidenceEvents
    let run_id_opt = profile.run_ids.last().cloned();

    let mut mapper = EvidenceMapper::new(run_id_opt, &profile.name);
    let events = mapper.map_profile(&profile, args.detail)?;
    let run_id = mapper.run_id().to_string();

    // 3. Write Bundle
    let out_path = args
        .out
        .unwrap_or_else(|| std::path::PathBuf::from(format!("assay_evidence_{}.tar.gz", run_id)));

    let out_file = File::create(&out_path)
        .with_context(|| format!("failed to create output file {}", out_path.display()))?;

    let mut bw = assay_evidence::bundle::BundleWriter::new(out_file);
    for ev in events {
        bw.add_event(ev);
    }

    bw.finish().context("failed to finalize evidence bundle")?;

    eprintln!("Exported evidence bundle to {}", out_path.display());
    Ok(0)
}

fn cmd_verify(args: EvidenceVerifyArgs) -> Result<i32> {
    if args.bundle.to_string_lossy() == "-" {
        let mut buf = Vec::new();
        io::stdin().read_to_end(&mut buf)?;
        assay_evidence::bundle::verify_bundle(io::Cursor::new(buf))
            .context("bundle verification failed")?;
        eprintln!("Bundle verified (stdin): OK");
        return Ok(0);
    }

    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    // BundleReader::open verifies by default
    let _ = assay_evidence::bundle::BundleReader::open(f)?;

    eprintln!("Bundle verified ({}): OK", args.bundle.display());
    Ok(0)
}

fn cmd_show(args: EvidenceShowArgs) -> Result<i32> {
    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let br = if args.no_verify {
        assay_evidence::bundle::BundleReader::open_unverified(f)
    } else {
        assay_evidence::bundle::BundleReader::open(f)
    }
    .context("failed to open bundle reader")?;

    let verified = !args.no_verify; // If open() succeeded above, it IS verified.
    let manifest = br.manifest();

    if args.format == "json" {
        // Output complete bundle as JSON: manifest + events
        let events = br.events().collect::<Result<Vec<_>>>()?;
        let bundle_json = serde_json::json!({
            "manifest": manifest,
            "events": events,
        });
        println!("{}", serde_json::to_string_pretty(&bundle_json)?);
        return Ok(0);
    }

    // Table view
    println!("Evidence Bundle Inspector");
    println!("=========================");
    if !args.no_verify {
        if verified {
            println!("Verified:    ✅ OK");
        } else {
            println!("Verified:    ❌ FAILED (Integrity compromised)");
        }
    } else {
        println!("Verified:    ⚠️  SKIPPED");
    }
    println!("Bundle ID:   {}", manifest.bundle_id);
    println!(
        "Producer:    {} v{}",
        manifest.producer.name, manifest.producer.version
    );
    println!("Run ID:      {}", manifest.run_id);
    println!("Events:      {}", manifest.event_count);
    let run_root_display: String = manifest.run_root.chars().take(16).collect();
    println!("Run Root:    {}...", run_root_display);
    println!();
    println!("{:<4} {:<25} {:<30} SUBJECT", "SEQ", "TIME", "TYPE");
    println!("{:-<4} {:-<25} {:-<30} {:-<20}", "", "", "", "");

    for ev_res in br.events() {
        let ev = ev_res?;
        let subject = ev.subject.as_deref().unwrap_or("-");
        let time_str = ev.time.to_rfc3339();
        let time_short = if time_str.len() > 19 {
            time_str.chars().skip(11).take(8).collect::<String>()
        } else {
            time_str.clone()
        };

        println!(
            "{:<4} {:<25} {:<30} {}",
            ev.seq, time_short, ev.type_, subject
        );
    }

    if !args.no_verify {
        println!("\n✅ Verified Integrity");
    } else {
        println!("\n⚠️  Verification Skipped");
    }

    Ok(0)
}
