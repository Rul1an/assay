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

pub fn require_regular_policy_target(
    metadata: &std::fs::Metadata,
    label: &str,
) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!("refusing to operate on symlinked policy target: {label}");
        }
    }
    if metadata.file_type().is_symlink() {
        anyhow::bail!("refusing to operate on symlinked policy target: {label}");
    }
    if !metadata.is_file() {
        anyhow::bail!("policy target {label} is not a regular file");
    }
    Ok(())
}

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
        use std::os::windows::io::{AsRawHandle, FromRawHandle};
        use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
        use windows_sys::Wdk::Storage::FileSystem::{
            NtCreateFile, FILE_NON_DIRECTORY_FILE, FILE_OPEN, FILE_OPEN_REPARSE_POINT,
            FILE_SYNCHRONOUS_IO_NONALERT,
        };
        use windows_sys::Win32::Foundation::{
            HANDLE, OBJ_CASE_INSENSITIVE, STATUS_NO_SUCH_FILE, STATUS_OBJECT_NAME_NOT_FOUND,
            STATUS_OBJECT_PATH_NOT_FOUND, UNICODE_STRING,
        };
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_GENERIC_READ, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let root_file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(root)
            .map_err(|error| {
                anyhow::anyhow!("failed to open policy root {}: {error}", root.display())
            })?;
        let root_metadata = root_file.metadata()?;
        if root_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!(
                "refusing to open reparse-point policy root {}",
                root.display()
            );
        }
        if !root_metadata.is_dir() {
            anyhow::bail!("policy root is not a directory: {}", root.display());
        }

        let mut wide_name: Vec<u16> = name.encode_utf16().collect();
        let byte_len = wide_name
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|len| u16::try_from(len).ok())
            .ok_or_else(|| anyhow::anyhow!("policy target name is too long"))?;
        let unicode_name = UNICODE_STRING {
            Length: byte_len,
            MaximumLength: byte_len,
            Buffer: wide_name.as_mut_ptr(),
        };
        let attributes = OBJECT_ATTRIBUTES {
            Length: std::mem::size_of::<OBJECT_ATTRIBUTES>() as u32,
            RootDirectory: root_file.as_raw_handle() as HANDLE,
            ObjectName: &unicode_name,
            Attributes: OBJ_CASE_INSENSITIVE,
            SecurityDescriptor: std::ptr::null(),
            SecurityQualityOfService: std::ptr::null(),
        };
        let mut target_handle: HANDLE = std::ptr::null_mut();
        let mut io_status = IO_STATUS_BLOCK::default();
        // SAFETY: all pointers refer to initialized values that remain alive
        // for the call, and a successful returned handle is transferred once
        // into `File` below.
        let status = unsafe {
            NtCreateFile(
                &mut target_handle,
                FILE_GENERIC_READ,
                &attributes,
                &mut io_status,
                std::ptr::null(),
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                FILE_OPEN,
                FILE_NON_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
                std::ptr::null(),
                0,
            )
        };
        if matches!(
            status,
            STATUS_NO_SUCH_FILE | STATUS_OBJECT_NAME_NOT_FOUND | STATUS_OBJECT_PATH_NOT_FOUND
        ) {
            return Ok(None);
        }
        if status < 0 || target_handle.is_null() {
            anyhow::bail!(
                "failed to open active policy target '{name}' relative to policy root: NTSTATUS {status:#010x}"
            );
        }
        // SAFETY: `NtCreateFile` returned a new owned handle on success, and
        // ownership is moved immediately into `File` for exactly one close.
        let target_file = unsafe { File::from_raw_handle(target_handle as _) };
        require_regular_policy_target(&target_file.metadata()?, name)?;
        read_bounded_file(target_file).map(Some)
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
