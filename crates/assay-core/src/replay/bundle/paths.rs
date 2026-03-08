use anyhow::Result;

/// Manifest at bundle root.
pub const MANIFEST: &str = "manifest.json";
/// Inputs (config, trace, etc.).
pub const FILES_PREFIX: &str = "files/";
/// Outputs (run.json, summary.json, sarif, junit).
pub const OUTPUTS_PREFIX: &str = "outputs/";
/// Scrubbed VCR/cassettes.
pub const CASSETTES_PREFIX: &str = "cassettes/";

/// Validates and normalizes a bundle entry path. Fail-closed: returns Ok(normalized) or Err.
///
/// Applies only to entry paths (files under files/, outputs/, cassettes/). The manifest file
/// (`manifest.json`) is written via bundle writer and never goes through this validator.
///
/// Rules: POSIX (backslash -> slash, no leading slash); no empty path or empty segments
/// (e.g. `files//x` rejected); no segment "." or ".." (segment check, so `files/a..b.txt`
/// is allowed); no drive letter (':' in first segment); canonical prefix required:
/// files/, outputs/, or cassettes/.
pub(crate) fn validate_entry_path(path: &str) -> Result<String> {
    let normalized = path.replace('\\', "/").trim_start_matches('/').to_string();
    if normalized.is_empty() {
        anyhow::bail!("invalid bundle path: empty path");
    }
    let segments: Vec<&str> = normalized.split('/').collect();
    if segments[0].contains(':') {
        anyhow::bail!(
            "invalid bundle path: drive-letter or ':' in first segment (path: {})",
            path
        );
    }
    for seg in &segments {
        if seg.is_empty() {
            anyhow::bail!("invalid bundle path: empty segment (path: {})", path);
        }
        if *seg == "." || *seg == ".." {
            anyhow::bail!(
                "invalid bundle path: traversal segment '.' or '..' (path: {})",
                path
            );
        }
    }
    let has_canonical_prefix = normalized.starts_with(FILES_PREFIX)
        || normalized.starts_with(OUTPUTS_PREFIX)
        || normalized.starts_with(CASSETTES_PREFIX);
    if !has_canonical_prefix {
        anyhow::bail!(
            "invalid bundle path prefix: must be files/, outputs/, or cassettes/ (path: {})",
            path
        );
    }
    Ok(normalized)
}
