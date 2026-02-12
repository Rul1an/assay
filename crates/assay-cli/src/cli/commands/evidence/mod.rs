pub mod diff;
pub mod lint;
pub mod list;
pub mod mapping;
pub mod pull;
pub mod push;

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
    /// Lint a bundle for quality and security issues
    Lint(lint::LintArgs),
    /// Diff two bundles and report changes
    Diff(diff::DiffArgs),
    /// Upload a bundle to remote storage (BYOS)
    Push(push::PushArgs),
    /// Download a bundle from remote storage (BYOS)
    Pull(pull::PullArgs),
    /// List bundles in remote storage (BYOS)
    List(list::ListArgs),
    /// Interactive TUI explorer for evidence bundles
    #[cfg(feature = "tui")]
    Explore(explore::ExploreArgs),
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
    /// Bundle path, or "-" for stdin. Required when using --eval.
    #[arg(value_name = "BUNDLE", default_value = "-")]
    pub bundle: std::path::PathBuf,

    /// Evaluation sidecar to verify against bundle (ADR-025 E2 Phase 3)
    #[arg(long, value_name = "PATH")]
    pub eval: Option<std::path::PathBuf>,

    /// Packs to resolve for digest verification (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub pack: Option<Vec<String>>,

    /// Fail if pack digests cannot be verified
    #[arg(long)]
    pub strict: bool,

    /// Machine-readable JSON output
    #[arg(long)]
    pub json: bool,

    /// Only exit code, no output
    #[arg(long, short = 'q')]
    pub quiet: bool,
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
        EvidenceCmd::Lint(a) => lint::cmd_lint(a),
        EvidenceCmd::Diff(a) => diff::cmd_diff(a),
        // BYOS commands (async)
        EvidenceCmd::Push(a) => tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime")
            .block_on(push::cmd_push(a)),
        EvidenceCmd::Pull(a) => tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime")
            .block_on(pull::cmd_pull(a)),
        EvidenceCmd::List(a) => tokio::runtime::Runtime::new()
            .expect("failed to create tokio runtime")
            .block_on(list::cmd_list(a)),
        #[cfg(feature = "tui")]
        EvidenceCmd::Explore(a) => explore::cmd_explore(a),
    }
}

fn cmd_export(args: EvidenceExportArgs) -> Result<i32> {
    // 1. Load Profile
    let profile = crate::cli::commands::profile_types::load_profile(&args.profile)
        .with_context(|| format!("failed to load profile from {}", args.profile.display()))?;

    // 2. Map Profile -> EvidenceEvents
    let run_id_opt = profile.run_ids.back().cloned();

    let mut mapper = EvidenceMapper::new(run_id_opt, &profile.name);
    let events = mapper.map_profile(&profile, args.detail)?;
    let run_id = mapper.run_id().to_string();

    // 3. Write Bundle
    let out_path = args
        .out
        .unwrap_or_else(|| std::path::PathBuf::from(format!("assay_evidence_{}.tar.gz", run_id)));

    let out_file = File::create(&out_path)
        .with_context(|| format!("failed to create output file {}", out_path.display()))?;

    let provenance = assay_evidence::ProvenanceInput {
        producer_name: "assay-cli".into(),
        producer_version: env!("CARGO_PKG_VERSION").into(),
        git_commit: option_env!("ASSAY_GIT_SHA").map(String::from),
        dirty: None,
        run_id: run_id.clone(),
        created_at: None, // use first event time (deterministic)
    };
    let mut bw = assay_evidence::bundle::BundleWriter::new(out_file).with_provenance(provenance);
    for ev in events {
        bw.add_event(ev);
    }

    bw.finish().context("failed to finalize evidence bundle")?;

    eprintln!("Exported evidence bundle to {}", out_path.display());
    Ok(0)
}

fn cmd_verify(args: EvidenceVerifyArgs) -> Result<i32> {
    if let Some(eval_path) = &args.eval {
        return cmd_verify_eval(&args, eval_path);
    }

    // Legacy: bundle-only verify
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

    let _ = assay_evidence::bundle::BundleReader::open(f)?;

    eprintln!("Bundle verified ({}): OK", args.bundle.display());
    Ok(0)
}

fn cmd_verify_eval(args: &EvidenceVerifyArgs, eval_path: &std::path::Path) -> Result<i32> {
    use assay_evidence::evaluation::verify_evaluation;
    use assay_evidence::lint::packs::load_packs;

    if args.bundle.to_string_lossy() == "-" {
        anyhow::bail!("--bundle is required when using --eval (cannot use stdin)");
    }

    let eval_json = std::fs::read_to_string(eval_path)
        .with_context(|| format!("failed to read evaluation {}", eval_path.display()))?;
    let evaluation: assay_evidence::evaluation::Evaluation =
        serde_json::from_str(&eval_json).context("invalid evaluation JSON")?;

    let f = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;
    let verify_result =
        assay_evidence::bundle::verify_bundle(f).context("bundle verification failed")?;
    let manifest = verify_result.manifest;

    let pack_digests = if let Some(ref pack_refs) = args.pack {
        match load_packs(pack_refs) {
            Ok(packs) => Some(
                packs
                    .iter()
                    .map(|p| {
                        (
                            format!("{}@{}", p.definition.name, p.definition.version),
                            p.digest.clone(),
                        )
                    })
                    .collect(),
            ),
            Err(e) => {
                if args.strict {
                    anyhow::bail!("pack resolution failed: {}", e);
                }
                None
            }
        }
    } else {
        None
    };

    let result = verify_evaluation(&evaluation, &manifest, pack_digests, args.strict)
        .context("evaluation verification failed")?;

    if args.quiet {
        return Ok(if result.ok { 0 } else { 1 });
    }

    if args.json {
        let out = serde_json::json!({
            "ok": result.ok,
            "bundle": {
                "bundle_digest_match": result.bundle_digest_match,
                "manifest_digest_match": result.manifest_digest_match,
            },
            "results": {
                "results_digest_verified": result.results_digest_verified,
                "results_digest_verifiable": result.results_digest_verifiable,
            },
            "packs": {
                "verified": result.packs_verified,
                "unverifiable": result.packs_unverifiable,
                "mismatched": result.packs_mismatched,
            },
            "warnings": result.warnings,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(if result.ok { 0 } else { 1 });
    }

    if result.ok {
        eprintln!("✅ Evaluation verified");
        eprintln!("   bundle_digest: match");
        eprintln!("   manifest_digest: match");
        eprintln!(
            "   results_digest: {}",
            if result.results_digest_verified {
                "verified"
            } else if result.results_digest_verifiable {
                "mismatch (see errors)"
            } else {
                "not verifiable (no report_inline)"
            }
        );
        eprintln!(
            "   packs: {} ok / {} unverifiable / {} mismatched",
            result.packs_verified, result.packs_unverifiable, result.packs_mismatched
        );
        for w in &result.warnings {
            eprintln!("   ⚠️  {}", w);
        }
        Ok(0)
    } else {
        eprintln!("❌ Evaluation verification failed");
        for e in &result.errors {
            eprintln!("   {}", e);
        }
        for w in &result.warnings {
            eprintln!("   ⚠️  {}", w);
        }
        Ok(1)
    }
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

// TUI explore module (conditional compilation)
#[cfg(feature = "tui")]
pub mod explore;
