use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use assay_core::agentic::SuggestedPatch;
use similar::TextDiff;

pub(super) fn apply_patch_to_file(patch: &SuggestedPatch) -> anyhow::Result<()> {
    let path = PathBuf::from(&patch.file);
    assay_core::fix::apply_ops_to_file(&path, &patch.ops)
        .with_context(|| format!("failed to apply patch {}", patch.id))?;
    Ok(())
}

pub(super) fn preview_patch(patch: &SuggestedPatch) -> anyhow::Result<()> {
    let path = PathBuf::from(&patch.file);
    let before = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read {}", path.display()))?;

    let is_json = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.eq_ignore_ascii_case("json"))
        .unwrap_or(false);

    let after = assay_core::fix::apply_ops_to_text(&before, &patch.ops, is_json)
        .with_context(|| format!("failed to preview patch {}", patch.id))?;

    print_unified_diff(&patch.file, &patch.id, &before, &after);
    Ok(())
}

pub(super) fn create_empty_trace(path: &Path) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }
    std::fs::write(path, "")
        .with_context(|| format!("failed to create trace file {}", path.display()))
}

pub(super) fn print_unified_diff(file: &str, patch_id: &str, before: &str, after: &str) {
    println!("--- {} (dry-run) patch={} ---", file, patch_id);

    if before == after {
        println!("(no changes)");
        println!("--- end ---");
        return;
    }

    let diff = TextDiff::from_lines(before, after);
    print!(
        "{}",
        diff.unified_diff().context_radius(3).header(file, file)
    );
    println!("--- end ---");
}

pub(super) fn write_text_file(path: &Path, content: &str) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        let parent = path.parent().unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("assay-config");
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let tmp = parent.join(format!(
            ".{}.assay-tmp-{}-{}",
            name,
            std::process::id(),
            nonce
        ));

        std::fs::write(&tmp, content)
            .with_context(|| format!("failed to write temp file {}", tmp.display()))?;
        std::fs::rename(&tmp, path).with_context(|| {
            format!(
                "failed to atomically replace {} with {}",
                path.display(),
                tmp.display()
            )
        })?;
        Ok(())
    }

    #[cfg(not(unix))]
    {
        std::fs::write(path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}
