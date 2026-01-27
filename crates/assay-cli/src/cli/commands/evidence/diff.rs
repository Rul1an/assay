use anyhow::{Context, Result};
use assay_evidence::diff::engine::diff_bundles;
use assay_evidence::VerifyLimits;
use clap::Args;
use std::fs::File;

#[derive(Debug, Args, Clone)]
pub struct DiffArgs {
    /// Baseline bundle (or candidate if --baseline-dir is used)
    #[arg(value_name = "BASELINE")]
    pub baseline: std::path::PathBuf,

    /// Candidate bundle
    #[arg(value_name = "CANDIDATE")]
    pub candidate: Option<std::path::PathBuf>,

    /// Output format: human or json
    #[arg(long, default_value = "human")]
    pub format: String,

    /// Baseline directory (look for {dir}/{key}.tar.gz)
    #[arg(long)]
    pub baseline_dir: Option<std::path::PathBuf>,

    /// Baseline key name (used with --baseline-dir)
    #[arg(long)]
    pub key: Option<String>,

    /// Write the candidate as the new baseline (creates {baseline-dir}/{key}.tar.gz)
    #[arg(long)]
    pub write_baseline: bool,

    /// Overwrite existing baseline (required if baseline already exists)
    #[arg(long)]
    pub update_baseline: bool,
}

pub fn cmd_diff(args: DiffArgs) -> Result<i32> {
    // Handle --write-baseline / --update-baseline before diffing
    if args.write_baseline || args.update_baseline {
        return cmd_write_baseline(&args);
    }

    let (baseline_path, candidate_path) = resolve_paths(&args)?;

    let baseline_file = File::open(&baseline_path)
        .with_context(|| format!("failed to open baseline {}", baseline_path.display()))?;
    let candidate_file = File::open(&candidate_path)
        .with_context(|| format!("failed to open candidate {}", candidate_path.display()))?;

    let limits = VerifyLimits::default();
    let report = diff_bundles(baseline_file, candidate_file, limits)?;

    match args.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        _ => {
            eprintln!("Assay Evidence Diff");
            eprintln!("===================");
            eprintln!(
                "Baseline:  {} ({} events)",
                report.baseline.run_id, report.baseline.event_count
            );
            eprintln!(
                "Candidate: {} ({} events)",
                report.candidate.run_id, report.candidate.event_count
            );
            eprintln!("Event count delta: {:+}", report.summary.event_count_delta);
            eprintln!();

            print_diff_set("Network", &report.network);
            print_diff_set("Filesystem", &report.filesystem);
            print_diff_set("Processes", &report.processes);

            if report.is_empty() {
                eprintln!("No differences found.");
            }
        }
    }

    Ok(0)
}

fn cmd_write_baseline(args: &DiffArgs) -> Result<i32> {
    let dir = args
        .baseline_dir
        .as_ref()
        .context("--write-baseline requires --baseline-dir")?;
    let key = args
        .key
        .as_ref()
        .context("--write-baseline requires --key")?;

    // Path safety: reject keys that could escape the baseline directory
    validate_baseline_key(key)?;

    // Canonicalize dir (after creating it) and ensure target stays within it
    std::fs::create_dir_all(dir)
        .with_context(|| format!("failed to create baseline dir {}", dir.display()))?;
    let canonical_dir = dir
        .canonicalize()
        .with_context(|| format!("failed to canonicalize baseline dir {}", dir.display()))?;
    let canonical_target = canonical_dir.join(format!("{}.tar.gz", key));
    if !canonical_target.starts_with(&canonical_dir) {
        anyhow::bail!("baseline key '{}' would escape baseline directory", key);
    }

    // Safety: don't overwrite unless --update-baseline is set
    if canonical_target.exists() && !args.update_baseline {
        anyhow::bail!(
            "Baseline already exists at {}. Use --update-baseline to overwrite.",
            canonical_target.display()
        );
    }

    // The candidate is the first positional arg in --baseline-dir mode
    let candidate_path = &args.baseline;

    // Verify the candidate bundle before writing it as a baseline
    let candidate_file = File::open(candidate_path)
        .with_context(|| format!("failed to open candidate {}", candidate_path.display()))?;
    let _ = assay_evidence::bundle::BundleReader::open(candidate_file)
        .context("candidate bundle verification failed — refusing to write as baseline")?;

    // Atomic write: copy to temp file in same dir, then rename
    let tmp_path = canonical_dir.join(format!(".{}.tar.gz.tmp", key));
    std::fs::copy(candidate_path, &tmp_path).with_context(|| {
        format!(
            "failed to copy {} to temp {}",
            candidate_path.display(),
            tmp_path.display()
        )
    })?;
    std::fs::rename(&tmp_path, &canonical_target).with_context(|| {
        format!(
            "failed to rename {} to {}",
            tmp_path.display(),
            canonical_target.display()
        )
    })?;

    eprintln!("Baseline written to {}", canonical_target.display());
    Ok(0)
}

/// Validate that a baseline key is safe for use as a filename component.
///
/// Rejects path separators, parent directory traversal, and other unsafe patterns.
fn validate_baseline_key(key: &str) -> Result<()> {
    if key.is_empty() {
        anyhow::bail!("baseline key must not be empty");
    }
    if key.contains('/') || key.contains('\\') {
        anyhow::bail!(
            "baseline key '{}' contains path separators — this is not allowed",
            key
        );
    }
    if key.contains("..") {
        anyhow::bail!(
            "baseline key '{}' contains '..' — path traversal is not allowed",
            key
        );
    }
    if key.starts_with('.') {
        anyhow::bail!(
            "baseline key '{}' starts with '.' — hidden files are not allowed",
            key
        );
    }
    // Reject any control characters
    if key.chars().any(|c| c.is_control()) {
        anyhow::bail!("baseline key contains control characters — this is not allowed");
    }
    Ok(())
}

fn resolve_paths(args: &DiffArgs) -> Result<(std::path::PathBuf, std::path::PathBuf)> {
    if let (Some(dir), Some(key)) = (&args.baseline_dir, &args.key) {
        validate_baseline_key(key)?;
        // Canonicalize dir to resolve symlinks, then verify containment.
        // This prevents a symlinked baseline-dir from escaping its apparent location.
        let canonical_dir = dir
            .canonicalize()
            .with_context(|| format!("failed to canonicalize baseline dir {}", dir.display()))?;
        let baseline_path = canonical_dir.join(format!("{}.tar.gz", key));
        if !baseline_path.starts_with(&canonical_dir) {
            anyhow::bail!("baseline key '{}' would escape baseline directory", key);
        }
        // --baseline-dir mode: first positional arg is the candidate
        let candidate_path = args.baseline.clone();
        Ok((baseline_path, candidate_path))
    } else {
        // Two-positional mode
        let candidate = args
            .candidate
            .as_ref()
            .context("candidate bundle path is required (or use --baseline-dir + --key)")?;
        Ok((args.baseline.clone(), candidate.clone()))
    }
}

fn print_diff_set(category: &str, diff: &assay_evidence::diff::DiffSet) {
    if diff.is_empty() {
        return;
    }
    eprintln!("{}:", category);
    for added in &diff.added {
        eprintln!("  + {}", added);
    }
    for removed in &diff.removed {
        eprintln!("  - {}", removed);
    }
    eprintln!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_baseline_key_rejects_path_traversal() {
        assert!(validate_baseline_key("../pwn").is_err());
        assert!(validate_baseline_key("..").is_err());
        assert!(validate_baseline_key("foo/../bar").is_err());
    }

    #[test]
    fn test_validate_baseline_key_rejects_path_separators() {
        assert!(validate_baseline_key("a/b").is_err());
        assert!(validate_baseline_key("a\\b").is_err());
        assert!(validate_baseline_key("/absolute").is_err());
        assert!(validate_baseline_key("\\windows").is_err());
    }

    #[test]
    fn test_validate_baseline_key_rejects_hidden_files() {
        assert!(validate_baseline_key(".hidden").is_err());
        assert!(validate_baseline_key(".").is_err());
    }

    #[test]
    fn test_validate_baseline_key_rejects_empty() {
        assert!(validate_baseline_key("").is_err());
    }

    #[test]
    fn test_validate_baseline_key_rejects_control_chars() {
        assert!(validate_baseline_key("foo\x00bar").is_err());
        assert!(validate_baseline_key("foo\nbar").is_err());
    }

    #[test]
    fn test_validate_baseline_key_accepts_valid() {
        assert!(validate_baseline_key("my-baseline").is_ok());
        assert!(validate_baseline_key("test_v2").is_ok());
        assert!(validate_baseline_key("2024-01-15-nightly").is_ok());
    }
}
