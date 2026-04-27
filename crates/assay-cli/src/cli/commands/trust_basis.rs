use crate::cli::args::{
    OutputFormat, TrustBasisArgs, TrustBasisDiffArgs, TrustBasisGenerateArgs, TrustBasisSub,
};
use crate::exit_codes::{EXIT_SUCCESS, EXIT_TEST_FAILURE};
use anyhow::{bail, Context, Result};
use assay_evidence::lint::engine::LintOptions;
use assay_evidence::lint::packs::load_packs;
use assay_evidence::{
    diff_trust_basis, duplicate_trust_basis_claim_ids, generate_trust_basis,
    to_canonical_json_bytes, TrustBasis, TrustBasisClaimLevelDiff, TrustBasisClaimMetadataDiff,
    TrustBasisClaimPresenceDiff, TrustBasisDiffReport, TrustBasisOptions, TrustClaimBoundary,
    TrustClaimId, TrustClaimLevel, TrustClaimSource, VerifyLimits,
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
    ensure_unique_claim_ids("baseline", &baseline)?;
    ensure_unique_claim_ids("candidate", &candidate)?;
    let report = diff_trust_basis(&baseline, &candidate);

    match args.format {
        OutputFormat::Text => write_diff_text(&report).context("failed to write diff output")?,
        OutputFormat::Json => write_diff_json(&report).context("failed to write diff output")?,
    }

    if args.fail_on_regression && report.has_regressions() {
        eprintln!("Trust Basis regression check failed");
        return Ok(EXIT_TEST_FAILURE);
    }

    Ok(EXIT_SUCCESS)
}

fn read_trust_basis(path: &std::path::Path) -> Result<TrustBasis> {
    let file = File::open(path)
        .with_context(|| format!("failed to open trust basis {}", path.display()))?;
    serde_json::from_reader(file)
        .with_context(|| format!("failed to parse trust basis {}", path.display()))
}

fn ensure_unique_claim_ids(label: &str, trust_basis: &TrustBasis) -> Result<()> {
    let duplicates = duplicate_trust_basis_claim_ids(trust_basis);
    if duplicates.is_empty() {
        return Ok(());
    }

    let duplicate_labels = duplicates
        .into_iter()
        .map(id_label)
        .collect::<Vec<_>>()
        .join(", ");
    bail!("{label} Trust Basis contains duplicate claim id(s): {duplicate_labels}");
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
    writeln!(stdout, "Claim identity: {}", report.claim_identity)?;
    if !report.has_changes() {
        writeln!(stdout, "No Trust Basis differences found.")?;
        return Ok(());
    }

    write_level_diffs(&mut stdout, "Regressions", &report.regressed_claims)?;
    write_level_diffs(&mut stdout, "Improvements", &report.improved_claims)?;
    write_presence_diffs(&mut stdout, "Removed claims", &report.removed_claims)?;
    write_presence_diffs(&mut stdout, "Added claims", &report.added_claims)?;
    write_metadata_diffs(&mut stdout, &report.metadata_changes)?;
    writeln!(stdout, "Unchanged claims: {}", report.unchanged_claim_count)?;
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
            id_label(diff.claim_id),
            level_label(diff.baseline_level),
            level_label(diff.candidate_level)
        )?;
    }
    Ok(())
}

fn write_presence_diffs(
    writer: &mut impl Write,
    title: &str,
    diffs: &[TrustBasisClaimPresenceDiff],
) -> Result<()> {
    if diffs.is_empty() {
        return Ok(());
    }

    writeln!(writer, "{title}:")?;
    for diff in diffs {
        writeln!(
            writer,
            "- {}: {} ({}, {})",
            id_label(diff.claim_id),
            optional_level_label(diff.baseline_level.or(diff.candidate_level)),
            optional_source_label(diff.baseline_source.or(diff.candidate_source)),
            optional_boundary_label(diff.baseline_boundary.or(diff.candidate_boundary))
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
            "- {}: source {} -> {}, boundary {} -> {}, note changed: {}",
            id_label(diff.claim_id),
            source_label(diff.baseline_source),
            source_label(diff.candidate_source),
            boundary_label(diff.baseline_boundary),
            boundary_label(diff.candidate_boundary),
            diff.note_changed
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

fn optional_level_label(level: Option<TrustClaimLevel>) -> String {
    level
        .map(level_label)
        .unwrap_or_else(|| "absent".to_string())
}

fn optional_source_label(source: Option<TrustClaimSource>) -> String {
    source
        .map(source_label)
        .unwrap_or_else(|| "unknown-source".to_string())
}

fn optional_boundary_label(boundary: Option<TrustClaimBoundary>) -> String {
    boundary
        .map(boundary_label)
        .unwrap_or_else(|| "unknown-boundary".to_string())
}

fn json_label(value: impl serde::Serialize) -> String {
    serde_json::to_string(&value)
        .expect("trust basis labels should serialize")
        .trim_matches('"')
        .to_string()
}
