use anyhow::Context;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

/// Resolve a bundle entry path against the workspace, refusing anything that could land outside
/// it.
///
/// The reader already normalizes and validates every entry path, so in a correct build nothing
/// reaches here that this would reject. That is the point: `workspace.join(rel)` silently returns
/// `rel` when `rel` is absolute, discarding the workspace entirely, so a single reader regression
/// turns into writes at the filesystem root — and under container or privileged execution that is
/// the whole host. The cost of checking here is a handful of comparisons per entry; the cost of
/// not checking is that the containment property depends on a caller two crates away staying
/// correct forever.
pub(super) fn resolve_in_workspace(workspace: &Path, rel: &str) -> anyhow::Result<PathBuf> {
    use std::path::Component;

    let candidate = Path::new(rel);
    anyhow::ensure!(
        candidate.is_relative(),
        "refusing bundle entry that is not workspace-relative"
    );

    // Lexical, not `canonicalize`: the target does not exist yet, and resolving what does exist
    // would follow symlinks we would rather simply not create. Every component must be a plain
    // name — no traversal, no root, no prefix.
    for component in candidate.components() {
        match component {
            Component::Normal(_) => {}
            _ => anyhow::bail!("refusing bundle entry with a non-literal path component"),
        }
    }

    let target = workspace.join(candidate);
    anyhow::ensure!(
        target.starts_with(workspace),
        "refusing bundle entry that resolves outside the replay workspace"
    );
    Ok(target)
}

pub(super) fn write_entries(workspace: &Path, entries: &[(String, Vec<u8>)]) -> anyhow::Result<()> {
    for (rel, data) in entries {
        let target = resolve_in_workspace(workspace, rel)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target, data)?;
    }
    Ok(())
}

pub(super) struct ReplayWorkspace {
    path: PathBuf,
}

impl ReplayWorkspace {
    pub(super) fn new() -> anyhow::Result<Self> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("clock before UNIX_EPOCH")?
            .as_nanos();
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("assay-replay-{}-{}", pid, now));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ReplayWorkspace {
    fn drop(&mut self) {
        if let Err(err) = std::fs::remove_dir_all(&self.path) {
            if err.kind() != ErrorKind::NotFound {
                eprintln!(
                    "warning: failed to clean replay workspace {}: {}",
                    self.path.display(),
                    err
                );
            }
        }
    }
}
