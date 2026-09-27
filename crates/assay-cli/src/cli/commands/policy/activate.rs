//! `assay policy activate` — activate a validated policy into a policy root.

use std::path::{Path, PathBuf};

use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};

use crate::cli::args::PolicyActivateArgs;
use crate::exit_codes;

pub const SCHEMA_ACTIVATION_V0: &str = "assay.policy.activation.v0";
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
pub const MAX_ACTIVATION_ENTRIES: usize = 10_000;
const MAX_TEMP_CLEANUP: usize = 64;
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ActivationRecord {
    pub schema: String,
    pub name: String,
    pub input_sha256: String,
    pub policy_digest: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_input_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_policy_digest: Option<String>,
    pub assay_version: String,
    pub activated_at: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_of: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PolicyRootDirs {
    pub root: PathBuf,
    #[allow(dead_code)]
    pub assay_dir: PathBuf,
    pub store_dir: PathBuf,
    pub activations_dir: PathBuf,
}

pub fn now_rfc3339_utc() -> String {
    chrono::Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn validate_target_name(name: &str) -> anyhow::Result<()> {
    let bytes = name.as_bytes();
    let portable = !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && !name.ends_with('.');
    let stem = name.split('.').next().unwrap_or("");
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'));
    if !portable || reserved {
        anyhow::bail!(
            "invalid policy target name '{name}': expected a portable filename of 1-128 ASCII letters, digits, '.', '_' or '-', beginning with a letter or digit, not ending in '.', and not a Windows device name"
        );
    }
    Ok(())
}

pub fn refuse_case_variant(dirs: &PolicyRootDirs, name: &str) -> anyhow::Result<()> {
    for (dir, records) in [(&dirs.root, false), (&dirs.activations_dir, true)] {
        if !dir.exists() {
            continue;
        }
        let mut count = 0_usize;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            count += 1;
            if count > MAX_ACTIVATION_ENTRIES {
                anyhow::bail!("policy directory {} exceeds entry ceiling", dir.display());
            }
            let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let candidate = if records {
                file_name
                    .strip_suffix(".json")
                    .and_then(|stem| stem.split_once('-'))
                    .filter(|(seq, _)| !seq.is_empty() && seq.bytes().all(|b| b.is_ascii_digit()))
                    .map(|(_, target)| target)
            } else {
                Some(file_name.as_str())
            };
            if candidate.is_some_and(|target| target != name && target.eq_ignore_ascii_case(name)) {
                anyhow::bail!(
                    "policy target '{name}' collides by case with existing entry '{file_name}'"
                );
            }
        }
    }
    Ok(())
}

pub fn ensure_policy_root_dirs(
    root: &Path,
    create_if_missing: bool,
) -> anyhow::Result<PolicyRootDirs> {
    let _root_meta = match std::fs::symlink_metadata(root) {
        Ok(m) => m,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("policy root directory does not exist: {}", root.display());
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "failed to access policy root {}: {err}",
                root.display()
            ));
        }
    };

    if !root.is_dir() {
        anyhow::bail!("policy root is not a directory: {}", root.display());
    }

    let canonical_root = root.canonicalize().map_err(|err| {
        anyhow::anyhow!(
            "failed to canonicalize policy root {}: {err}",
            root.display()
        )
    })?;

    let assay_dir = root.join(".assay");
    let store_dir = assay_dir.join("policy-store");
    let activations_dir = assay_dir.join("activations");

    // 1. Check / create .assay
    match std::fs::symlink_metadata(&assay_dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "refusing to operate on symlinked assay directory: {}",
                    assay_dir.display()
                );
            }
            if !meta.is_dir() {
                anyhow::bail!(
                    "assay directory is not a directory: {}",
                    assay_dir.display()
                );
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if create_if_missing {
                std::fs::create_dir(&assay_dir).map_err(|err| {
                    anyhow::anyhow!(
                        "failed to create assay directory {}: {err}",
                        assay_dir.display()
                    )
                })?;
            }
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "failed to access assay directory {}: {err}",
                assay_dir.display()
            ));
        }
    }

    // 2. Check / create policy-store
    match std::fs::symlink_metadata(&store_dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "refusing to operate on symlinked policy-store directory: {}",
                    store_dir.display()
                );
            }
            if !meta.is_dir() {
                anyhow::bail!(
                    "policy-store directory is not a directory: {}",
                    store_dir.display()
                );
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if create_if_missing {
                std::fs::create_dir(&store_dir).map_err(|err| {
                    anyhow::anyhow!(
                        "failed to create policy-store directory {}: {err}",
                        store_dir.display()
                    )
                })?;
            }
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "failed to access policy-store directory {}: {err}",
                store_dir.display()
            ));
        }
    }

    // 3. Check / create activations
    match std::fs::symlink_metadata(&activations_dir) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                anyhow::bail!(
                    "refusing to operate on symlinked activations directory: {}",
                    activations_dir.display()
                );
            }
            if !meta.is_dir() {
                anyhow::bail!(
                    "activations directory is not a directory: {}",
                    activations_dir.display()
                );
            }
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if create_if_missing {
                std::fs::create_dir(&activations_dir).map_err(|err| {
                    anyhow::anyhow!(
                        "failed to create activations directory {}: {err}",
                        activations_dir.display()
                    )
                })?;
            }
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "failed to access activations directory {}: {err}",
                activations_dir.display()
            ));
        }
    }

    // 4. Verify canonical containment
    if assay_dir.exists() {
        let canonical_assay = assay_dir.canonicalize().map_err(|err| {
            anyhow::anyhow!(
                "failed to canonicalize assay dir {}: {err}",
                assay_dir.display()
            )
        })?;
        if !canonical_assay.starts_with(&canonical_root) {
            anyhow::bail!(
                "assay directory {} escapes root {}",
                assay_dir.display(),
                root.display()
            );
        }
    }

    if store_dir.exists() {
        let canonical_store = store_dir.canonicalize().map_err(|err| {
            anyhow::anyhow!(
                "failed to canonicalize policy-store dir {}: {err}",
                store_dir.display()
            )
        })?;
        if !canonical_store.starts_with(&canonical_root) {
            anyhow::bail!(
                "policy-store directory {} escapes root {}",
                store_dir.display(),
                root.display()
            );
        }
    }

    if activations_dir.exists() {
        let canonical_activations = activations_dir.canonicalize().map_err(|err| {
            anyhow::anyhow!(
                "failed to canonicalize activations dir {}: {err}",
                activations_dir.display()
            )
        })?;
        if !canonical_activations.starts_with(&canonical_root) {
            anyhow::bail!(
                "activations directory {} escapes root {}",
                activations_dir.display(),
                root.display()
            );
        }
    }

    Ok(PolicyRootDirs {
        root: root.to_path_buf(),
        assay_dir,
        store_dir,
        activations_dir,
    })
}

pub fn store_policy_content(
    store_dir: &Path,
    input_sha256: &str,
    bytes: &[u8],
) -> anyhow::Result<PathBuf> {
    let file_name = store_object_filename(input_sha256)?;
    let path = match assay_common::atomic_write::write_new(store_dir, &file_name, bytes) {
        Ok(path) => path,
        Err(assay_common::atomic_write::WriteNewError::AlreadyExists { .. }) => {
            store_dir.join(&file_name)
        }
        Err(err) => {
            return Err(anyhow::anyhow!(
                "failed to store policy content in policy-store: {err}"
            ));
        }
    };
    if read_store_object(store_dir, input_sha256)? != bytes {
        anyhow::bail!("policy-store object changed during verification");
    }
    Ok(path)
}

fn parse_activation_record_seq(
    record_name: &str,
    target_name: &str,
) -> anyhow::Result<Option<u64>> {
    let suffix = format!("-{target_name}.json");
    let Some(prefix) = record_name.strip_suffix(&suffix) else {
        return Ok(None);
    };
    if prefix.is_empty() || !prefix.bytes().all(|b| b.is_ascii_digit()) {
        return Ok(None);
    }
    let seq = prefix.parse::<u64>().map_err(|_| {
        anyhow::anyhow!("activation record '{record_name}' has an overflowing sequence")
    })?;
    if format!("{seq:06}-{target_name}.json") != record_name {
        anyhow::bail!("activation record '{record_name}' has a noncanonical sequence");
    }
    Ok(Some(seq))
}

fn is_lower_hex_64(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn validate_store_object_name(name: &str) -> anyhow::Result<()> {
    let Some(hex) = name.strip_prefix("sha256:") else {
        anyhow::bail!(
            "invalid policy-store object name '{name}': expected sha256:<64 lowercase hex chars>"
        );
    };
    if !is_lower_hex_64(hex) || hex.bytes().any(|b| b.is_ascii_uppercase()) {
        anyhow::bail!(
            "invalid policy-store object name '{name}': expected sha256:<64 lowercase hex chars>"
        );
    }
    Ok(())
}

/// The digest is a logical identity; `:` is not portable as a filename.
pub fn store_object_filename(identity: &str) -> anyhow::Result<String> {
    validate_store_object_name(identity)?;
    Ok(format!("sha256-{}", &identity["sha256:".len()..]))
}

pub fn read_store_object(store_dir: &Path, object_name: &str) -> anyhow::Result<Vec<u8>> {
    let file_name = store_object_filename(object_name)?;
    let bytes = match super::resolved::read_active_bounded(store_dir, &file_name)? {
        Some(bytes) => bytes,
        None => {
            #[cfg(unix)]
            {
                // Roots created by v0 used the digest string as a POSIX name.
                super::resolved::read_active_bounded(store_dir, object_name)?.ok_or_else(|| {
                    anyhow::anyhow!("policy-store object '{object_name}' does not exist")
                })?
            }
            #[cfg(not(unix))]
            {
                anyhow::bail!("policy-store object '{object_name}' does not exist");
            }
        }
    };
    if super::resolved::input_sha256(&bytes) != object_name {
        anyhow::bail!("policy-store object '{object_name}' has a digest mismatch");
    }
    Ok(bytes)
}

pub fn validate_pointer_target(root: &Path, name: &str) -> anyhow::Result<Option<PathBuf>> {
    validate_target_name(name)?;
    let target = root.join(name);

    match std::fs::symlink_metadata(&target) {
        Ok(meta) => {
            super::resolved::require_regular_policy_target(
                &meta,
                target.to_string_lossy().as_ref(),
            )?;
            Ok(Some(target))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(anyhow::anyhow!(
            "failed to inspect policy target {}: {err}",
            target.display()
        )),
    }
}

#[cfg(debug_assertions)]
fn dry_run_open_barrier_wait() -> anyhow::Result<()> {
    let barrier = match std::env::var("ASSAY_TEST_DRY_RUN_OPEN_BARRIER") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => return Ok(()),
    };
    let arrival = barrier.join(format!("arrived-{}", std::process::id()));
    std::fs::write(&arrival, b"").map_err(|error| {
        anyhow::anyhow!(
            "test-only dry-run open barrier could not write {}: {error}",
            arrival.display()
        )
    })?;
    let release = barrier.join("release");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    while !release.exists() {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "test-only dry-run open barrier timed out in {}",
                barrier.display()
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}

pub fn replace_pointer_atomic(root: &Path, name: &str, bytes: &[u8]) -> anyhow::Result<PathBuf> {
    let target = validate_pointer_target(root, name)?.unwrap_or_else(|| root.join(name));

    let temp_name = format!(
        ".{name}.tmp.{}.{:x}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let temp_path = root.join(&temp_name);

    {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|err| {
                anyhow::anyhow!(
                    "failed to create temporary file {}: {err}",
                    temp_path.display()
                )
            })?;
        use std::io::Write;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    #[cfg(debug_assertions)]
    policy_failpoint("pointer-staged");

    #[cfg(windows)]
    let rename_result = {
        let mut result = std::fs::rename(&temp_path, &target);
        for _ in 0..4 {
            if !result
                .as_ref()
                .is_err_and(|err| err.kind() == std::io::ErrorKind::PermissionDenied)
            {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
            result = std::fs::rename(&temp_path, &target);
        }
        result
    };
    #[cfg(not(windows))]
    let rename_result = std::fs::rename(&temp_path, &target);
    if let Err(err) = rename_result {
        let _ = std::fs::remove_file(&temp_path);
        return Err(anyhow::anyhow!(
            "failed to replace {} via atomic rename: {err}",
            target.display()
        ));
    }
    #[cfg(debug_assertions)]
    policy_failpoint("pointer-renamed");

    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let dir_file = std::fs::File::open(root)?;
        nix::unistd::fsync(dir_file.as_raw_fd()).map_err(|err| {
            anyhow::anyhow!("committed pointer pending: failed to sync policy root: {err}")
        })?;
    }
    #[cfg(debug_assertions)]
    policy_failpoint("pointer-durable");

    Ok(target)
}

pub fn find_latest_activation_record(
    activations_dir: &Path,
    name: &str,
) -> anyhow::Result<Option<(u64, String, ActivationRecord)>> {
    find_activation_record_before(activations_dir, name, None)
}

pub fn find_activation_record_before(
    activations_dir: &Path,
    name: &str,
    upper_exclusive: Option<u64>,
) -> anyhow::Result<Option<(u64, String, ActivationRecord)>> {
    if !activations_dir.exists() {
        return Ok(None);
    }
    let mut highest: Option<(u64, String)> = None;
    let mut entry_count = 0_usize;

    for entry in std::fs::read_dir(activations_dir)? {
        let entry = entry?;
        entry_count += 1;
        if entry_count > MAX_ACTIVATION_ENTRIES {
            anyhow::bail!("activation directory exceeds {MAX_ACTIVATION_ENTRIES} entries");
        }
        let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(seq) = parse_activation_record_seq(&file_name, name)? else {
            continue;
        };
        if upper_exclusive.is_some_and(|upper| seq >= upper) {
            continue;
        }
        if !entry.file_type()?.is_file() {
            anyhow::bail!("activation record '{file_name}' is not a regular file");
        }

        if highest.as_ref().is_none_or(|(max_seq, _)| seq > *max_seq) {
            highest = Some((seq, file_name));
        }
    }

    let Some((seq, file_name)) = highest else {
        return Ok(None);
    };
    let content = super::resolved::read_regular_at_bounded(
        activations_dir,
        &file_name,
        MAX_RECORD_BYTES as u64,
    )?
    .ok_or_else(|| anyhow::anyhow!("activation record '{file_name}' disappeared during read"))?;
    let rec: ActivationRecord = serde_json::from_slice(&content)
        .map_err(|err| anyhow::anyhow!("invalid activation record '{file_name}': {err}"))?;
    if rec.name != name || rec.schema != SCHEMA_ACTIVATION_V0 {
        anyhow::bail!("invalid activation record '{file_name}': name or schema mismatch");
    }

    Ok(Some((seq, file_name, rec)))
}

pub fn validate_head_predecessor(
    activations_dir: &Path,
    name: &str,
    head_seq: u64,
    head: &ActivationRecord,
) -> anyhow::Result<Option<(u64, String, ActivationRecord)>> {
    if head_seq == 1 {
        if head.previous_input_sha256.is_some() || head.previous_policy_digest.is_some() {
            anyhow::bail!("activation history is inconsistent: first record has a predecessor");
        }
        return Ok(None);
    }
    let predecessor = find_activation_record_before(activations_dir, name, Some(head_seq))?
        .ok_or_else(|| {
            anyhow::anyhow!("activation history is inconsistent: missing predecessor")
        })?;
    if predecessor.0.checked_add(1) != Some(head_seq)
        || head.previous_input_sha256.as_deref() != Some(predecessor.2.input_sha256.as_str())
        || head.previous_policy_digest.as_deref() != Some(predecessor.2.policy_digest.as_str())
        || head
            .rollback_of
            .as_deref()
            .is_some_and(|rollback_of| rollback_of != predecessor.1)
    {
        anyhow::bail!("activation history is inconsistent: predecessor does not match head");
    }
    Ok(Some(predecessor))
}

/// A single root-scoped kernel lock serializes all policy commits and recovery.
/// The lock file is retained: unlinking it could let two processes lock
/// different inodes under the same pathname.
pub fn acquire_policy_lock(dirs: &PolicyRootDirs) -> anyhow::Result<std::fs::File> {
    #[cfg(unix)]
    let file = {
        use nix::fcntl::{open, openat, OFlag};
        use nix::sys::stat::Mode;
        use std::os::fd::FromRawFd;
        let dir_fd = open(
            &dirs.assay_dir,
            OFlag::O_RDONLY | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::empty(),
        )?;
        let lock_fd = openat(
            dir_fd,
            "lock",
            OFlag::O_RDWR | OFlag::O_CREAT | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            Mode::from_bits_truncate(0o600),
        );
        let close_result = nix::unistd::close(dir_fd);
        let lock_fd = lock_fd?;
        close_result?;
        // SAFETY: openat returned an owned descriptor, transferred exactly once.
        unsafe { std::fs::File::from_raw_fd(lock_fd) }
    };
    #[cfg(windows)]
    let file = {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(dirs.assay_dir.join("lock"))?;
        if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            anyhow::bail!("refusing reparse-point policy lock");
        }
        file
    };
    let deadline = std::time::Instant::now() + LOCK_WAIT;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(std::fs::TryLockError::WouldBlock) => {
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!("policy root is busy: lock wait exceeded {LOCK_WAIT:?}");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(err) => return Err(anyhow::anyhow!("failed to lock policy root: {err}")),
        }
    }
}

/// Under the root lock, bring the active file forward to the last committed
/// record. A foreign active value is never overwritten.
pub fn recover_committed_head(
    dirs: &PolicyRootDirs,
    name: &str,
) -> anyhow::Result<Option<(u64, String, ActivationRecord)>> {
    cleanup_transaction_temps(dirs, name)?;
    let head = find_latest_activation_record(&dirs.activations_dir, name)?;
    let active = super::resolved::read_active_bounded(&dirs.root, name)?;
    let Some((seq, _, record)) = &head else {
        if active.is_some() {
            anyhow::bail!("active policy '{name}' is unrecorded; refusing recovery");
        }
        return Ok(None);
    };
    validate_head_predecessor(&dirs.activations_dir, name, *seq, record)?;
    let committed_bytes = read_store_object(&dirs.store_dir, &record.input_sha256)?;
    let committed = super::resolved::load_resolved(&committed_bytes)?;
    if committed.policy_digest != record.policy_digest {
        anyhow::bail!("committed policy '{name}' has a policy digest mismatch");
    }
    match active {
        Some(bytes) if bytes == committed_bytes => {}
        Some(bytes)
            if record.previous_input_sha256.as_deref()
                == Some(super::resolved::input_sha256(&bytes).as_str()) =>
        {
            replace_pointer_atomic(&dirs.root, name, &committed_bytes)?;
        }
        None => {
            replace_pointer_atomic(&dirs.root, name, &committed_bytes)?;
        }
        Some(_) => anyhow::bail!("foreign active policy '{name}'; refusing recovery"),
    }
    Ok(head)
}

fn cleanup_transaction_temps(dirs: &PolicyRootDirs, name: &str) -> anyhow::Result<()> {
    let mut candidates = Vec::new();
    for (dir, kind) in [
        (&dirs.root, 0_u8),
        (&dirs.store_dir, 1),
        (&dirs.activations_dir, 2),
    ] {
        if !dir.exists() {
            continue;
        }
        let mut entries = 0_usize;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            entries += 1;
            if entries > MAX_ACTIVATION_ENTRIES {
                anyhow::bail!("policy directory {} exceeds entry ceiling", dir.display());
            }
            let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let is_ours = is_transaction_temp(&file_name, name, kind);
            if !is_ours {
                continue;
            }
            if !entry.file_type()?.is_file() {
                anyhow::bail!("refusing non-file transaction temp '{file_name}'");
            }
            candidates.push(entry.path());
            if candidates.len() > MAX_TEMP_CLEANUP {
                anyhow::bail!("too many stale policy transaction temps");
            }
        }
    }
    for path in candidates {
        std::fs::remove_file(&path).map_err(|err| {
            anyhow::anyhow!("failed to remove stale temp {}: {err}", path.display())
        })?;
    }
    Ok(())
}

fn is_transaction_temp(file_name: &str, name: &str, kind: u8) -> bool {
    let Some((base, suffix)) = file_name
        .strip_prefix('.')
        .and_then(|rest| rest.rsplit_once(".tmp."))
    else {
        return false;
    };
    let parts: Vec<_> = suffix.split('.').collect();
    let expected = if kind == 0 { 2 } else { 3 };
    if parts.len() != expected
        || parts[0].is_empty()
        || parts[1].is_empty()
        || !parts[0].bytes().all(|byte| byte.is_ascii_hexdigit())
        || !parts[1].bytes().all(|byte| byte.is_ascii_hexdigit())
        || (expected == 3
            && (parts[2].is_empty() || !parts[2].bytes().all(|byte| byte.is_ascii_digit())))
    {
        return false;
    }
    match kind {
        0 => base == name && parts[0].bytes().all(|byte| byte.is_ascii_digit()),
        1 => {
            let hex = base
                .strip_prefix("sha256-")
                .or_else(|| base.strip_prefix("sha256:"));
            hex.is_some_and(|hex| {
                is_lower_hex_64(hex) && !hex.bytes().any(|b| b.is_ascii_uppercase())
            })
        }
        _ => parse_activation_record_seq(base, name)
            .ok()
            .flatten()
            .is_some(),
    }
}

#[cfg(debug_assertions)]
pub fn policy_failpoint(point: &str) {
    if std::env::var("ASSAY_TEST_POLICY_FAILPOINT").as_deref() == Ok(point) {
        std::process::abort();
    }
}

/// Test-only rendezvous before the transaction lock for the concurrent test.
///
/// Both processes reach the barrier before either takes the lock, then the
/// lock serializes their commits. Never set outside that test.
///
/// Compiled out of release builds (`debug_assertions` off), following
/// `crates/assay-mcp-server/src/tools/check_args.rs`: the env var has no
/// effect on the release binary.
#[cfg(debug_assertions)]
fn race_barrier_wait() -> anyhow::Result<()> {
    let barrier = match std::env::var("ASSAY_TEST_ACTIVATE_RACE_BARRIER") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => return Ok(()),
    };
    let parties: usize = std::env::var("ASSAY_TEST_ACTIVATE_RACE_PARTIES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2);
    let arrival = barrier.join(format!(
        "arrived-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0)
    ));
    // Best-effort marker: the test verifies both arrivals landed, so a
    // silently skipped barrier cannot pass as exercised retry coverage.
    if std::fs::write(&arrival, b"").is_err() {
        return Ok(());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        let arrived = std::fs::read_dir(&barrier).map(|entries| entries.count());
        match arrived {
            Ok(count) if count >= parties => return Ok(()),
            _ => {
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!(
                        "test-only activation race barrier timed out waiting for {parties} arrivals in {}",
                        barrier.display()
                    );
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn write_activation_record(
    activations_dir: &Path,
    name: &str,
    input_sha256: &str,
    policy_digest: &str,
    previous_input_sha256: Option<String>,
    previous_policy_digest: Option<String>,
    source: &str,
    rollback_of: Option<String>,
) -> anyhow::Result<(ActivationRecord, PathBuf)> {
    let latest = find_latest_activation_record(activations_dir, name)?;
    let next_seq = latest.as_ref().map_or(1, |(seq, _, _)| *seq + 1);
    let record_name = format!("{:06}-{name}.json", next_seq);

    let record = ActivationRecord {
        schema: SCHEMA_ACTIVATION_V0.to_string(),
        name: name.to_string(),
        input_sha256: input_sha256.to_string(),
        policy_digest: policy_digest.to_string(),
        previous_input_sha256: previous_input_sha256.clone(),
        previous_policy_digest: previous_policy_digest.clone(),
        assay_version: env!("CARGO_PKG_VERSION").to_string(),
        activated_at: now_rfc3339_utc(),
        source: source.to_string(),
        rollback_of: rollback_of.clone(),
    };

    let record_bytes = serde_json::to_vec_pretty(&record)?;
    if record_bytes.len() > MAX_RECORD_BYTES {
        anyhow::bail!("activation record exceeds {MAX_RECORD_BYTES} bytes");
    }

    let path = assay_common::atomic_write::write_new(activations_dir, &record_name, &record_bytes)
        .map_err(|err| {
            anyhow::anyhow!("failed to commit activation record {record_name}: {err}")
        })?;
    Ok((record, path))
}

pub async fn run(args: PolicyActivateArgs) -> anyhow::Result<i32> {
    let name = args.target_name()?;
    validate_target_name(&name)?;

    // 1. Read and validate source policy before touching active state
    let bytes = super::resolved::read_bounded(&args.src)
        .map_err(|error| super::classify_load_error(&args.src, error))?;
    let resolved = super::resolved::load_resolved(&bytes)
        .map_err(|error| super::classify_load_error(&args.src, error))?;

    if args.dry_run {
        let dirs = ensure_policy_root_dirs(&args.root, false)?;
        #[cfg(debug_assertions)]
        dry_run_open_barrier_wait()?;
        let current = match super::resolved::read_active_bounded(&dirs.root, &name)? {
            Some(active_bytes) => Some(super::resolved::load_resolved(&active_bytes).map_err(
                |error| {
                    anyhow::anyhow!(
                        "active policy {} failed to validate: {error}",
                        dirs.root.join(&name).display()
                    )
                },
            )?),
            None => None,
        };
        let first_activation = current.is_none();
        let byte_change = current
            .as_ref()
            .is_none_or(|active| active.input_sha256 != resolved.input_sha256);
        let semantic_change = current
            .as_ref()
            .is_none_or(|active| active.policy_digest != resolved.policy_digest);
        let current_input = current
            .as_ref()
            .map_or("none", |active| active.input_sha256.as_str());
        let current_digest = current
            .as_ref()
            .map_or("none", |active| active.policy_digest.as_str());
        eprintln!(
            "Policy activation preview: name={name} first_activation={first_activation} byte_change={byte_change} semantic_change={semantic_change} current_input_sha256={current_input} proposed_input_sha256={} current_policy_digest={current_digest} proposed_policy_digest={} writes=none",
            resolved.input_sha256, resolved.policy_digest
        );
        return Ok(exit_codes::OK);
    }

    // 2. Ensure .assay directories exist inside root (fail if root missing or symlinked)
    let dirs = ensure_policy_root_dirs(&args.root, true)?;

    #[cfg(debug_assertions)]
    race_barrier_wait()?;
    let _lock = acquire_policy_lock(&dirs)?;
    #[cfg(debug_assertions)]
    policy_failpoint("lock-acquired");
    refuse_case_variant(&dirs, &name)?;
    let head = recover_committed_head(&dirs, &name)?;

    // 3. Store content in content store
    #[cfg(debug_assertions)]
    policy_failpoint("before-store");
    store_policy_content(&dirs.store_dir, &resolved.input_sha256, &bytes)?;
    #[cfg(debug_assertions)]
    policy_failpoint("store-committed");

    // 4. Bind the predecessor while holding the same root lock.
    let (prev_sha, prev_digest) = match head {
        Some((_, _, ref prev_rec)) => (
            Some(prev_rec.input_sha256.clone()),
            Some(prev_rec.policy_digest.clone()),
        ),
        None => (None, None),
    };

    // 5. Commit the record before changing the active file. A crash here is
    // repaired by recover_committed_head on the next mutating command.
    #[cfg(debug_assertions)]
    policy_failpoint("before-record");
    let source_str = args.src.display().to_string();
    let (record, record_path) = write_activation_record(
        &dirs.activations_dir,
        &name,
        &resolved.input_sha256,
        &resolved.policy_digest,
        prev_sha,
        prev_digest,
        &source_str,
        None,
    )?;
    #[cfg(debug_assertions)]
    policy_failpoint("record-committed");
    #[cfg(debug_assertions)]
    policy_failpoint("before-pointer");
    replace_pointer_atomic(&dirs.root, &name, &bytes)?;
    let actual = super::resolved::read_active_bounded(&dirs.root, &name)?;
    if actual.as_deref() != Some(bytes.as_slice()) {
        anyhow::bail!("committed pointer pending: active bytes differ after replacement");
    }

    eprintln!(
        "✔ Policy activated: {} (input: {}, digest: {}, record: {})",
        name,
        record.input_sha256,
        record.policy_digest,
        record_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    );

    Ok(exit_codes::OK)
}
