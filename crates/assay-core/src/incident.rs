use anyhow::{Context, Result, anyhow};
use assay_common::exports::{EventRecordExport, ProcessTreeExport};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, FromRawFd};
#[cfg(unix)]
use nix::fcntl::{open, openat, OFlag, renameat};
#[cfg(unix)]
use nix::sys::stat::{Mode, fchmod};

use uuid::Uuid;

#[cfg(not(unix))]
trait PermissionsExt {} // Stub

#[derive(Debug, Serialize)]
pub struct IncidentBundle {
    pub metadata: IncidentMetadata,
    pub tree: ProcessTreeExport,
    pub events: Vec<EventRecordExport>,
}

#[derive(Debug, Serialize)]
pub struct IncidentMetadata {
    pub timestamp: String,
    pub session_id: String,
    pub kernel_version: String,
    pub assay_version: String,
}

pub struct IncidentBuilder {
    bundle: IncidentBundle,
}

impl IncidentBuilder {
    pub fn new(session_id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        // Simple kernel version retrieval
        let kernel_version = std::fs::read_to_string("/proc/version")
            .unwrap_or_else(|_| "unknown".to_string())
            .trim()
            .to_string();

        Self {
            bundle: IncidentBundle {
                metadata: IncidentMetadata {
                    timestamp: now,
                    session_id,
                    kernel_version,
                    assay_version: env!("CARGO_PKG_VERSION").to_string(),
                },
                tree: ProcessTreeExport::default(),
                events: Vec::new(),
            }
        }
    }

    pub fn with_tree(mut self, tree: ProcessTreeExport) -> Self {
        self.bundle.tree = tree;
        self
    }

    pub fn with_events(mut self, events: Vec<EventRecordExport>) -> Self {
        self.bundle.events = events;
        self
    }

    /// Writes the bundle atomically to the specified directory.
    /// Creates a temp file, sets permissions (0600), then moves it.
    /// Enforces 0700 on parent directory for security.
    /// Secure Atomic Write (SOTA P0)
    /// Uses low-level O_NOFOLLOW/openat to prevent symlink attacks.
    /// Enforces 0700 on directory and 0600 on file.
    #[cfg(unix)]
    pub fn atomic_write(&self, output_dir: &Path) -> Result<PathBuf> {
        let dir_path_str = output_dir.to_str().ok_or_else(|| anyhow!("Invalid path"))?;

        // 1. Ensure directory exists
        if !output_dir.exists() {
             fs::create_dir_all(output_dir).context("Failed to create output dir")?;
             let mut perms = fs::metadata(output_dir)?.permissions();
             perms.set_mode(0o700);
             fs::set_permissions(output_dir, perms).context("Failed to secure new output dir")?;
        }

        // 2. Open Directory securely (O_DIRECTORY | O_NOFOLLOW)
        let dir_raw_fd = open(
            dir_path_str,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW,
            Mode::empty()
        ).context("Failed to open output directory securely")?;

        // SAFTEY: We wrap the raw FD immediately to ensure RAII closure.
        let dir_file = unsafe { std::fs::File::from_raw_fd(dir_raw_fd) };

        // 3. Verify Directory Permissions (fstat on fd)
        let dir_meta = dir_file.metadata()?;
        let current_mode = dir_meta.permissions().mode();
        if (current_mode & 0o777) != 0o700 {
            // P0: Enforce 0700 always (via fd to avoid TOCTOU)
            fchmod(dir_file.as_raw_fd(), Mode::from_bits_truncate(0o700))
                .context("Failed to fchmod output directory")?;
        }

        // SOTA: Guaranteed unique filename to prevent overwrites (collision free)
        let suffix = Uuid::new_v4().simple().to_string();
        let filename = format!("incident_{}_{}.json", self.bundle.metadata.session_id, suffix);
        let tmp_filename = format!(".tmp_{}", filename);

        let content = serde_json::to_string_pretty(&self.bundle)
            .context("Failed to serialize incident bundle")?;

        // 4. Open Temp File (openat relative to dir_fd, O_CREAT|O_EXCL|O_NOFOLLOW, 0600)
        let tmp_fd = openat(
            dir_file.as_raw_fd(),
            tmp_filename.as_str(),
            OFlag::O_CREAT | OFlag::O_WRONLY | OFlag::O_EXCL | OFlag::O_NOFOLLOW,
            Mode::from_bits_truncate(0o600)
        ).context("Failed to create temp file securely")?;

        let mut tmp_file = unsafe { std::fs::File::from_raw_fd(tmp_fd) };

        // 5. Write and Fsync
        use std::io::Write;
        tmp_file.write_all(content.as_bytes())?;
        tmp_file.sync_all()?;

        // 6. Atomic Rename (renameat)
        renameat(
            Some(dir_file.as_raw_fd()),
            tmp_filename.as_str(),
            Some(dir_file.as_raw_fd()),
            filename.as_str()
        ).context("Failed to rename atomic file")?;

        // 7. Sync Parent Directory
        dir_file.sync_all()?;

        Ok(output_dir.join(filename))
    }

    #[cfg(not(unix))]
    pub fn atomic_write(&self, _output_dir: &Path) -> Result<PathBuf> {
        Err(anyhow!("Incident bundles only supported on Unix"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn test_atomic_write_security() -> Result<()> {
        let temp_dir = tempfile::tempdir()?;
        let builder = IncidentBuilder::new("test-session".to_string());

        // Write info
        let path = builder.atomic_write(temp_dir.path())?;

        // Check 1: File exists
        assert!(path.exists());
        assert!(path.file_name().unwrap().to_str().unwrap().contains("test-session"));

        // Check 2: Permissions (0600)
        let perms = fs::metadata(&path)?.permissions();
        let mode = perms.mode() & 0o777;
        assert_eq!(mode, 0o600, "Incident bundle permissions must be 0600");

        // Check 3: Content
        let content = fs::read_to_string(&path)?;
        let json: serde_json::Value = serde_json::from_str(&content)?;
        assert_eq!(json["metadata"]["session_id"], "test-session");

        Ok(())
    }
}
