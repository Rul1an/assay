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

    /// Output bundle path (.tar.gz)
    #[arg(long, short = 'o')]
    pub out: std::path::PathBuf,

    /// Level of detail to include (summary, observed, full)
    #[arg(long, value_enum, default_value_t = DetailLevel::Observed)]
    pub detail: DetailLevel,
}

#[derive(Debug, Args, Clone)]
pub struct EvidenceVerifyArgs {
    /// Bundle path, or "-" for stdin
    #[arg(value_name = "BUNDLE", default_value = "-")]
    pub bundle: String,
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
    // Deterministic RunID: Use profile name + hash of created_at?
    // For now, let's use profile.name as base, or generate a fresh UUID if not stable.
    // Ideally we'd have a persistent RunID in the profile.
    // Profile struct has `run_ids` (vector). We could use the LAST run id?
    // Or just generating a "bundle run id".
    let run_id_opt = profile.run_ids.last().cloned();

    let mut mapper = EvidenceMapper::new(run_id_opt, &profile.name);
    let events = mapper.map_profile(&profile, args.detail)?;

    // 3. Write Bundle
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

fn cmd_show(args: EvidenceShowArgs) -> Result<i32> {
    use assay_evidence::bundle::reader::BundleReader;

    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    // Verify first (unless skipped)
    if !args.no_verify {
        let mut verify_reader = File::open(&args.bundle)?;
        assay_evidence::bundle::verify_bundle(&mut verify_reader)
            .context("Verification FAILED (use --no-verify to inspect corrupt bundles)")?;
    }

    // Now read contents
    let br = BundleReader::open(f)?;
    let manifest = br.manifest();

    if args.format == "json" {
        // Just dump all events as JSON array?
        // Or Manifest + Events?
        println!("{}", serde_json::to_string_pretty(&manifest)?);
        // Todo: iterate events and print
        return Ok(0);
    }

    // Table view
    println!("Evidence Bundle Inspector");
    println!("=========================");
    println!("Bundle ID:   {}", manifest.bundle_id);
    println!(
        "Producer:    {} v{}",
        manifest.producer.name, manifest.producer.version
    );
    println!("Run ID:      {}", manifest.run_id);
    println!("Events:      {}", manifest.event_count);
    println!("Run Root:    {}...", &manifest.run_root[..16]);
    println!("");
    println!("{:<4} {:<25} {:<30} SUBJECT", "SEQ", "TIME", "TYPE");
    println!("{:-<4} {:-<25} {:-<30} {:-<20}", "", "", "", "");

    for ev_res in br.events() {
        let ev = ev_res?;
        let subject = ev.subject.as_deref().unwrap_or("-");
        // Truncate time for display
        let time_str = ev.time.to_rfc3339();
        let time_short = if time_str.len() > 19 {
            &time_str[11..19]
        } else {
            &time_str
        };

        println!(
            "{:<4} {:<25} {:<30} {}",
            ev.seq,
            time_short, // Use full time or truncated? use full for now
            ev.type_,
            subject
        );
    }

    if !args.no_verify {
        println!("\n✅ Verified Integrity");
    } else {
        println!("\n⚠️  Verification Skipped");
    }

    Ok(0)
}
