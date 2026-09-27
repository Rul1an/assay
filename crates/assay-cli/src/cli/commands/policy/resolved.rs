//! Shared policy resolution and validation loader.
//!
//! Provides `load_resolved` and bounded reading for `validate`, `resolve`,
//! `activate`, and `status`.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use assay_common::limits::{LimitKind, LimitReader};
use assay_core::mcp::policy::McpPolicy;
use sha2::{Digest, Sha256};

pub const MAX_INPUT_BYTES: u64 = 1_000_000;

fn read_bounded_file(file: File) -> anyhow::Result<Vec<u8>> {
    let mut reader = LimitReader::new(file, MAX_INPUT_BYTES, LimitKind::SourceBytes);
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;
    Ok(buf)
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub policy: McpPolicy,
    pub input_sha256: String,
    pub policy_digest: String,
}

pub fn read_bounded(path: &Path) -> anyhow::Result<Vec<u8>> {
    read_bounded_file(File::open(path)?)
}

/// Open an active policy without following the final path component and bind
/// type validation plus the byte ceiling to that same opened handle.
pub fn read_active_bounded(root: &Path, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::fcntl::{open, openat, OFlag};
        use nix::sys::stat::Mode;
        use std::os::fd::{AsRawFd, FromRawFd};

        let root_fd = open(
            root,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| {
            anyhow::anyhow!("failed to open policy root {}: {error}", root.display())
        })?;
        // SAFETY: `open` returned an owned descriptor and ownership is moved
        // immediately into `File`, which closes it exactly once.
        let root_file = unsafe { File::from_raw_fd(root_fd) };
        let target_fd = match openat(
            root_file.as_raw_fd(),
            name,
            OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_NONBLOCK | OFlag::O_CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(Errno::ENOENT) => return Ok(None),
            Err(Errno::ELOOP) => {
                anyhow::bail!("refusing to read symlinked active policy target '{name}'")
            }
            Err(error) => anyhow::bail!("failed to open active policy target '{name}': {error}"),
        };
        // SAFETY: `openat` returned an owned descriptor and ownership is moved
        // immediately into `File`, which closes it exactly once.
        let target_file = unsafe { File::from_raw_fd(target_fd) };
        if !target_file.metadata()?.is_file() {
            anyhow::bail!("active policy target '{name}' is not a regular file");
        }
        read_bounded_file(target_file).map(Some)
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let path = root.join(name);
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => anyhow::bail!(
                "failed to open active policy target {}: {error}",
                path.display()
            ),
        };
        let metadata = file.metadata()?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!(
                "refusing to read symlinked active policy target {}",
                path.display()
            );
        }
        if !metadata.is_file() {
            anyhow::bail!(
                "active policy target {} is not a regular file",
                path.display()
            );
        }
        read_bounded_file(file).map(Some)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let path = root.join(name);
        match std::fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Ok(_) => anyhow::bail!(
                "secure active policy reads are unsupported on this platform: {}",
                path.display()
            ),
            Err(error) => Err(error.into()),
        }
    }
}

pub fn input_sha256(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

pub fn load_resolved(bytes: &[u8]) -> anyhow::Result<Resolved> {
    let policy = McpPolicy::from_slice(bytes)?;
    policy
        .try_compile_all_schemas()
        .map_err(|error| anyhow::anyhow!("policy schemas failed to compile: {error}"))?;
    let policy_digest = policy
        .policy_digest()
        .ok_or_else(|| anyhow::anyhow!("failed to canonicalize policy digest"))?;
    Ok(Resolved {
        policy,
        input_sha256: input_sha256(bytes),
        policy_digest,
    })
}
