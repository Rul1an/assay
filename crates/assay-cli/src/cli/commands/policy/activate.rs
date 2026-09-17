//! `assay policy activate` — activate a validated policy into a policy root.

use std::path::{Path, PathBuf};

use chrono::SecondsFormat;
use serde::{Deserialize, Serialize};

use crate::cli::args::PolicyActivateArgs;
use crate::exit_codes;

pub const SCHEMA_ACTIVATION_V0: &str = "assay.policy.activation.v0";
pub const MAX_RECORD_RETRIES: usize = 10;

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
    if name.is_empty() || name.contains('/') || name.contains('\\') || name == "." || name == ".." {
        anyhow::bail!(
            "invalid policy target name '{name}': name must be a single filename component without path separators"
        );
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
    match assay_common::atomic_write::write_new(store_dir, input_sha256, bytes) {
        Ok(path) => Ok(path),
        Err(assay_common::atomic_write::WriteNewError::AlreadyExists { .. }) => {
            Ok(store_dir.join(input_sha256))
        }
        Err(err) => Err(anyhow::anyhow!(
            "failed to store policy content in policy-store: {err}"
        )),
    }
}

pub fn replace_pointer_atomic(root: &Path, name: &str, bytes: &[u8]) -> anyhow::Result<PathBuf> {
    validate_target_name(name)?;
    let target = root.join(name);

    if let Ok(meta) = std::fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() {
            anyhow::bail!(
                "refusing to operate on symlinked policy target: {}",
                target.display()
            );
        }
        if meta.is_dir() {
            anyhow::bail!(
                "policy target {} is a directory; expected a regular file",
                target.display()
            );
        }
    }

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

    if let Err(err) = std::fs::rename(&temp_path, &target) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(anyhow::anyhow!(
            "failed to replace {} via atomic rename: {err}",
            target.display()
        ));
    }

    #[cfg(unix)]
    {
        if let Ok(dir_file) = std::fs::File::open(root) {
            use std::os::fd::AsRawFd;
            let _ = nix::unistd::fsync(dir_file.as_raw_fd());
        }
    }

    Ok(target)
}

pub fn find_latest_activation_record(
    activations_dir: &Path,
    name: &str,
) -> anyhow::Result<Option<(u64, String, ActivationRecord)>> {
    if !activations_dir.exists() {
        return Ok(None);
    }
    let suffix = format!("-{name}.json");
    let mut highest: Option<(u64, String, ActivationRecord)> = None;

    for entry in std::fs::read_dir(activations_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name.ends_with(&suffix) {
            let prefix = &file_name[..file_name.len() - suffix.len()];
            if let Ok(seq) = prefix.parse::<u64>() {
                let content = std::fs::read(entry.path())?;
                if let Ok(rec) = serde_json::from_slice::<ActivationRecord>(&content) {
                    if highest
                        .as_ref()
                        .is_none_or(|(max_seq, _, _)| seq > *max_seq)
                    {
                        highest = Some((seq, file_name, rec));
                    }
                }
            }
        }
    }

    Ok(highest)
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
    for _attempt in 0..MAX_RECORD_RETRIES {
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

        match assay_common::atomic_write::write_new(activations_dir, &record_name, &record_bytes) {
            Ok(path) => return Ok((record, path)),
            Err(assay_common::atomic_write::WriteNewError::AlreadyExists { .. }) => {
                // Sequence number race with another concurrent activation; retry
                continue;
            }
            Err(err) => {
                return Err(anyhow::anyhow!(
                    "failed to write activation record {record_name}: {err}"
                ));
            }
        }
    }

    Err(anyhow::anyhow!(
        "failed to write activation record for '{name}' after {MAX_RECORD_RETRIES} attempts due to concurrent activations"
    ))
}

pub async fn run(args: PolicyActivateArgs) -> anyhow::Result<i32> {
    let name = args.target_name()?;
    validate_target_name(&name)?;

    // 1. Read and validate source policy before touching active state
    let bytes = super::resolved::read_bounded(&args.src)
        .map_err(|error| super::classify_load_error(&args.src, error))?;
    let resolved = super::resolved::load_resolved(&bytes)
        .map_err(|error| super::classify_load_error(&args.src, error))?;

    // 2. Ensure .assay directories exist inside root (fail if root missing or symlinked)
    let dirs = ensure_policy_root_dirs(&args.root, true)?;

    // 3. Store content in content store
    store_policy_content(&dirs.store_dir, &resolved.input_sha256, &bytes)?;

    // 4. Find previous state
    let latest = find_latest_activation_record(&dirs.activations_dir, &name)?;
    let (prev_sha, prev_digest) = match latest {
        Some((_, _, ref prev_rec)) => (
            Some(prev_rec.input_sha256.clone()),
            Some(prev_rec.policy_digest.clone()),
        ),
        None => (None, None),
    };

    // 5. Replace active pointer atomically
    replace_pointer_atomic(&dirs.root, &name, &bytes)?;

    // 6. Record activation
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
