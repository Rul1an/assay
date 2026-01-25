use std::io;
use std::path::PathBuf;

/// Ensures the `.assay/traces` directory exists in the current working directory.
/// Returns the path to the trace directory.
pub fn ensure_assay_trace_dir() -> io::Result<PathBuf> {
    let cwd = std::env::current_dir()?;
    let trace_dir = cwd.join(".assay").join("traces");

    if !trace_dir.exists() {
        std::fs::create_dir_all(&trace_dir)?;
    }

    Ok(trace_dir)
}
