use crate::cli::args::{
    OutputFormat, TrustBasisArgs, TrustBasisDiffArgs, TrustBasisGenerateArgs, TrustBasisSub,
};
use crate::exit_codes::EXIT_SUCCESS;
use anyhow::{bail, Context, Result};
use assay_evidence::lint::engine::LintOptions;
use assay_evidence::lint::packs::load_packs;
use assay_evidence::{
    diff_trust_basis, generate_trust_basis, to_canonical_json_bytes, TrustBasis, TrustBasisClaim,
    TrustBasisClaimLevelDiff, TrustBasisClaimMetadataDiff, TrustBasisDiffReport, TrustBasisOptions,
    TrustClaimBoundary, TrustClaimId, TrustClaimLevel, TrustClaimSource, VerifyLimits,
};
use std::fs::File;
use std::io::Write;

pub fn run(args: TrustBasisArgs) -> Result<i32> {
    match args.cmd {
        TrustBasisSub::Generate(args) => cmd_generate(args),
        TrustBasisSub::Diff(args) => cmd_diff(args),
    }
}

fn cmd_generate(args: TrustBasisGenerateArgs) -> Result<i32> {
    let bundle = File::open(&args.bundle)
        .with_context(|| format!("failed to open bundle {}", args.bundle.display()))?;

    let lint = if let Some(pack_refs) = &args.pack {
        let packs = load_packs(pack_refs).context("failed to load trust-basis packs")?;
        Some(LintOptions {
            packs,
            max_results: Some(args.max_results),
            bundle_path: Some(args.bundle.display().to_string()),
        })
    } else {
        None
    };

    let trust_basis =
        generate_trust_basis(bundle, VerifyLimits::default(), TrustBasisOptions { lint })
            .context("failed to generate trust basis")?;

    let output =
        to_canonical_json_bytes(&trust_basis).context("failed to serialize trust basis")?;

    if let Some(out_path) = args.out {
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create trust-basis output directory {}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&out_path, output)
            .with_context(|| format!("failed to write trust basis to {}", out_path.display()))?;
        eprintln!("Wrote canonical trust basis to {}", out_path.display());
    } else {
        std::io::stdout()
            .write_all(&output)
            .context("failed to write trust basis to stdout")?;
    }

    Ok(EXIT_SUCCESS)
}

fn cmd_diff(args: TrustBasisDiffArgs) -> Result<i32> {
    let baseline = read_trust_basis(&args.baseline)?;
    let candidate = read_trust_basis(&args.candidate)?;
    let report = diff_trust_basis(&baseline, &candidate);

    match args.format {
        OutputFormat::Text => write_diff_text(&report).context("failed to write diff output")?,
        OutputFormat::Json => write_diff_json(&report).context("failed to write diff output")?,
    }

    if args.fail_on_regression && report.has_regressions() {
        bail!("Trust Basis regression check failed");
    }

    Ok(EXIT_SUCCESS)
}

fn read_trust_basis(path: &std::path::Path) -> Result<TrustBasis> {
    let file = File::open(path)
        .with_context(|| format!("failed to open trust basis {}", path.display()))?;
    serde_json::from_reader(file)
        .with_context(|| format!("failed to parse trust basis {}", path.display()))
}

fn write_diff_json(report: &TrustBasisDiffReport) -> Result<()> {
    let mut stdout = std::io::stdout();
    serde_json::to_writer_pretty(&mut stdout, report)?;
    stdout.write_all(b"\n")?;
    Ok(())
}

fn write_diff_text(report: &TrustBasisDiffReport) -> Result<()> {
    let mut stdout = std::io::stdout();
    writeln!(stdout, "Assay Trust Basis Diff")?;
    if !report.has_changes() {
        writeln!(stdout, "No Trust Basis differences found.")?;
        return Ok(());
    }

    write_level_diffs(&mut stdout, "Regressions", &report.regressions)?;
    write_level_diffs(&mut stdout, "Improvements", &report.improvements)?;
    write_claims(&mut stdout, "Removed claims", &report.removals)?;
    write_claims(&mut stdout, "Added claims", &report.additions)?;
    write_metadata_diffs(&mut stdout, &report.metadata_changes)?;
    writeln!(stdout, "Unchanged claims: {}", report.unchanged)?;
    Ok(())
}

fn write_level_diffs(
    writer: &mut impl Write,
    title: &str,
    diffs: &[TrustBasisClaimLevelDiff],
) -> Result<()> {
    if diffs.is_empty() {
        return Ok(());
    }

    writeln!(writer, "{title}:")?;
    for diff in diffs {
        writeln!(
            writer,
            "- {}: {} -> {}",
            id_label(diff.id),
            level_label(diff.baseline_level),
            level_label(diff.candidate_level)
        )?;
    }
    Ok(())
}

fn write_claims(writer: &mut impl Write, title: &str, claims: &[TrustBasisClaim]) -> Result<()> {
    if claims.is_empty() {
        return Ok(());
    }

    writeln!(writer, "{title}:")?;
    for claim in claims {
        writeln!(
            writer,
            "- {}: {} ({}, {})",
            id_label(claim.id),
            level_label(claim.level),
            source_label(claim.source),
            boundary_label(claim.boundary)
        )?;
    }
    Ok(())
}

fn write_metadata_diffs(
    writer: &mut impl Write,
    diffs: &[TrustBasisClaimMetadataDiff],
) -> Result<()> {
    if diffs.is_empty() {
        return Ok(());
    }

    writeln!(writer, "Metadata changes:")?;
    for diff in diffs {
        writeln!(
            writer,
            "- {}: source {} -> {}, boundary {} -> {}",
            id_label(diff.id),
            source_label(diff.baseline_source),
            source_label(diff.candidate_source),
            boundary_label(diff.baseline_boundary),
            boundary_label(diff.candidate_boundary)
        )?;
    }
    Ok(())
}

fn id_label(id: TrustClaimId) -> String {
    json_label(id)
}

fn level_label(level: TrustClaimLevel) -> String {
    json_label(level)
}

fn source_label(source: TrustClaimSource) -> String {
    json_label(source)
}

fn boundary_label(boundary: TrustClaimBoundary) -> String {
    json_label(boundary)
}

fn json_label(value: impl serde::Serialize) -> String {
    serde_json::to_string(&value)
        .expect("trust basis labels should serialize")
        .trim_matches('"')
        .to_string()
}
