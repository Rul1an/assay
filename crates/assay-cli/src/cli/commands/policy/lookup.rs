//! Bounded, offline lookup of exact retained policy bytes by semantic digest.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::cli::args::common::OutputFormat;
use crate::cli::args::PolicyLookupArgs;
use crate::exit_codes;

use super::activate::{self, ActivationRecord, PolicyRootDirs};
use super::resolved;

const SCHEMA_LOOKUP_V0: &str = "assay.policy.lookup.v0";
const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_BYTES_EXAMINED: u64 = 3 * MAX_SCAN_BYTES;

#[derive(Serialize, PartialEq, Eq)]
struct LookupReport {
    schema: &'static str,
    policy_digest: String,
    matches: Vec<LookupMatch>,
}

#[derive(Serialize, PartialEq, Eq)]
struct LookupMatch {
    policy_digest: String,
    input_sha256: String,
    byte_length: usize,
    store_object: String,
    store_filenames: Vec<String>,
    recorded: bool,
    activations: Vec<ActivationInfo>,
}

#[derive(Serialize, PartialEq, Eq)]
struct ActivationInfo {
    record_file: String,
    sequence: u64,
    name: String,
    activated_at: String,
    source: String,
    rollback_of: Option<String>,
}

struct StoredObject {
    bytes: Vec<u8>,
    policy_digest: String,
    filenames: Vec<String>,
    activations: Vec<ActivationInfo>,
}

fn read_entry_bounded(
    dir: &Path,
    entry: &std::fs::DirEntry,
    name: &str,
    per_file_max: u64,
    remaining: &mut u64,
) -> anyhow::Result<Vec<u8>> {
    if !entry.file_type()?.is_file() {
        anyhow::bail!("lookup candidate '{name}' is not a regular file");
    }
    let len = entry.metadata()?.len();
    if len > per_file_max || len > *remaining {
        anyhow::bail!("lookup candidate '{name}' exceeds byte ceiling");
    }
    let bytes = resolved::read_regular_at_bounded(dir, name, per_file_max.min(*remaining))?
        .ok_or_else(|| anyhow::anyhow!("changed during lookup: '{name}' disappeared"))?;
    *remaining = remaining
        .checked_sub(bytes.len() as u64)
        .ok_or_else(|| anyhow::anyhow!("lookup total byte ceiling exceeded"))?;
    Ok(bytes)
}

fn scan_store(
    dirs: &PolicyRootDirs,
    remaining: &mut u64,
) -> anyhow::Result<BTreeMap<String, StoredObject>> {
    let mut objects: BTreeMap<String, StoredObject> = BTreeMap::new();
    let mut entries = 0_usize;
    for entry in std::fs::read_dir(&dirs.store_dir)? {
        let entry = entry?;
        entries += 1;
        if entries > activate::MAX_ACTIVATION_ENTRIES {
            anyhow::bail!("policy-store exceeds entry ceiling");
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF-8 policy-store filename"))?
            .to_owned();
        let identity = activate::store_identity_from_filename(&name)?;
        let bytes = read_entry_bounded(
            &dirs.store_dir,
            &entry,
            &name,
            resolved::MAX_INPUT_BYTES,
            remaining,
        )?;
        let loaded = resolved::load_resolved(&bytes)?;
        if loaded.input_sha256 != identity {
            anyhow::bail!("policy-store object '{name}' has a digest mismatch");
        }
        match objects.get_mut(&identity) {
            Some(existing) => {
                if existing.bytes != bytes {
                    anyhow::bail!("conflicting policy-store aliases for '{identity}'");
                }
                existing.filenames.push(name);
            }
            None => {
                objects.insert(
                    identity,
                    StoredObject {
                        bytes,
                        policy_digest: loaded.policy_digest,
                        filenames: vec![name],
                        activations: Vec::new(),
                    },
                );
            }
        }
    }
    Ok(objects)
}

fn scan_records(
    dirs: &PolicyRootDirs,
    objects: &mut BTreeMap<String, StoredObject>,
    remaining: &mut u64,
) -> anyhow::Result<()> {
    let mut entries = 0_usize;
    let mut histories: BTreeMap<String, BTreeMap<u64, (String, ActivationRecord)>> =
        BTreeMap::new();
    for entry in std::fs::read_dir(&dirs.activations_dir)? {
        let entry = entry?;
        entries += 1;
        if entries > activate::MAX_ACTIVATION_ENTRIES {
            anyhow::bail!("activation directory exceeds entry ceiling");
        }
        let name = entry
            .file_name()
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("non-UTF-8 activation filename"))?
            .to_owned();
        let bytes = read_entry_bounded(
            &dirs.activations_dir,
            &entry,
            &name,
            activate::MAX_RECORD_BYTES as u64,
            remaining,
        )?;
        let record: ActivationRecord = serde_json::from_slice(&bytes)
            .map_err(|err| anyhow::anyhow!("invalid activation record '{name}': {err}"))?;
        let sequence = activate::validate_activation_record_identity(&name, &record)?;
        let object = objects.get(&record.input_sha256).ok_or_else(|| {
            anyhow::anyhow!("activation record '{name}' has no retained policy-store object")
        })?;
        if object.policy_digest != record.policy_digest {
            anyhow::bail!("activation record '{name}' has a policy digest mismatch");
        }
        let target = record.name.clone();
        if histories
            .entry(target)
            .or_default()
            .insert(sequence, (name, record))
            .is_some()
        {
            anyhow::bail!("duplicate activation sequence in policy history");
        }
    }
    for history in histories.values() {
        let mut predecessor: Option<(u64, &str, &ActivationRecord)> = None;
        for (&sequence, (file_name, record)) in history {
            activate::validate_predecessor_link(sequence, record, predecessor)?;
            let object = objects
                .get_mut(&record.input_sha256)
                .ok_or_else(|| anyhow::anyhow!("record object disappeared during lookup"))?;
            object.activations.push(ActivationInfo {
                record_file: file_name.clone(),
                sequence,
                name: record.name.clone(),
                activated_at: record.activated_at.clone(),
                source: record.source.clone(),
                rollback_of: record.rollback_of.clone(),
            });
            predecessor = Some((sequence, file_name, record));
        }
    }
    Ok(())
}

fn report_for_digest(
    dirs: &PolicyRootDirs,
    digest: &str,
    total_remaining: &mut u64,
) -> anyhow::Result<(LookupReport, BTreeMap<String, Vec<u8>>)> {
    let mut remaining = MAX_SCAN_BYTES.min(*total_remaining);
    let scan_limit = remaining;
    let mut objects = scan_store(dirs, &mut remaining)?;
    scan_records(dirs, &mut objects, &mut remaining)?;
    *total_remaining -= scan_limit - remaining;

    let mut matches = Vec::new();
    let mut matching_bytes = BTreeMap::new();
    for (identity, mut object) in objects {
        if object.policy_digest != digest {
            continue;
        }
        object.filenames.sort();
        object.activations.sort_by(|a, b| {
            (&a.name, a.sequence, &a.record_file).cmp(&(&b.name, b.sequence, &b.record_file))
        });
        let byte_length = object.bytes.len();
        matching_bytes.insert(identity.clone(), object.bytes);
        matches.push(LookupMatch {
            policy_digest: object.policy_digest,
            input_sha256: identity.clone(),
            byte_length,
            store_object: identity,
            store_filenames: object.filenames,
            recorded: !object.activations.is_empty(),
            activations: object.activations,
        });
    }
    Ok((
        LookupReport {
            schema: SCHEMA_LOOKUP_V0,
            policy_digest: digest.to_owned(),
            matches,
        },
        matching_bytes,
    ))
}

fn scan_with_lock_generation_check(
    dirs: &PolicyRootDirs,
    digest: &str,
    total_remaining: &mut u64,
    lock_held: bool,
) -> anyhow::Result<(LookupReport, BTreeMap<String, Vec<u8>>)> {
    let report = report_for_digest(dirs, digest, total_remaining);
    if !lock_held {
        match std::fs::symlink_metadata(dirs.assay_dir.join("lock")) {
            Ok(_) => anyhow::bail!("changed during lookup: policy transaction lock appeared"),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.into()),
        }
    }
    report
}

#[cfg(debug_assertions)]
fn scan_barrier(variable: &str, marker: &str) -> anyhow::Result<()> {
    let Ok(dir) = std::env::var(variable) else {
        return Ok(());
    };
    let barrier = Path::new(&dir);
    std::fs::write(barrier.join(marker), b"")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !barrier.join("continue").exists() {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!("test-only lookup barrier timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    Ok(())
}

fn write_export_atomic(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temp_path: Option<PathBuf> = None;
    let mut temp_file = None;
    for attempt in 0..8_u8 {
        let name = format!(
            ".assay-lookup-{}.{}.{attempt}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        );
        let candidate = parent.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                temp_path = Some(candidate);
                temp_file = Some(file);
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }
    let temp_path =
        temp_path.ok_or_else(|| anyhow::anyhow!("failed to allocate lookup output temp"))?;
    let mut file = temp_file.ok_or_else(|| anyhow::anyhow!("missing lookup output temp handle"))?;
    let result = (|| -> anyhow::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(debug_assertions)]
        if std::env::var_os("ASSAY_TEST_LOOKUP_FAIL_BEFORE_OUTPUT_RENAME").is_some() {
            anyhow::bail!("test-only lookup failure before output rename");
        }
        std::fs::rename(&temp_path, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    result
}

fn validate_output_location(dirs: &PolicyRootDirs, path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let canonical_parent = parent.canonicalize()?;
    let canonical_assay = dirs.assay_dir.canonicalize()?;
    if canonical_parent.starts_with(&canonical_assay) {
        anyhow::bail!("lookup output cannot replace policy-root metadata");
    }
    Ok(())
}

pub async fn run(args: PolicyLookupArgs) -> anyhow::Result<i32> {
    activate::validate_store_object_name(&args.policy_digest)
        .map_err(|err| anyhow::anyhow!("invalid policy digest: {err}"))?;
    if let Some(input) = &args.input_sha256 {
        activate::validate_store_object_name(input)
            .map_err(|err| anyhow::anyhow!("invalid input SHA-256 digest: {err}"))?;
    }
    let dirs = activate::ensure_policy_root_dirs(&args.root, false)?;
    // A present lock serializes completed writer transactions. The repeated
    // bounded scans also catch direct changes that do not respect that lock.
    let _lock = activate::acquire_existing_policy_lock(&dirs)?;
    let mut remaining = MAX_TOTAL_BYTES_EXAMINED;
    let first = scan_with_lock_generation_check(
        &dirs,
        &args.policy_digest,
        &mut remaining,
        _lock.is_some(),
    )?;
    #[cfg(debug_assertions)]
    scan_barrier("ASSAY_TEST_LOOKUP_AFTER_FIRST_SCAN", "first-scan-complete")?;
    let second = scan_with_lock_generation_check(
        &dirs,
        &args.policy_digest,
        &mut remaining,
        _lock.is_some(),
    )?;
    let (report, bytes) = if first == second {
        second
    } else {
        #[cfg(debug_assertions)]
        scan_barrier(
            "ASSAY_TEST_LOOKUP_AFTER_SECOND_SCAN",
            "second-scan-complete",
        )?;
        let third = scan_with_lock_generation_check(
            &dirs,
            &args.policy_digest,
            &mut remaining,
            _lock.is_some(),
        )?;
        if second != third {
            anyhow::bail!("changed during lookup: bounded snapshot retry did not stabilize");
        }
        third
    };
    if let Some(path) = &args.output {
        validate_output_location(&dirs, path)?;
        let selected = if let Some(identity) = &args.input_sha256 {
            bytes
                .get(identity)
                .ok_or_else(|| anyhow::anyhow!("input SHA-256 does not match this policy digest"))?
        } else if bytes.len() == 1 {
            bytes
                .values()
                .next()
                .ok_or_else(|| anyhow::anyhow!("no policy bytes match"))?
        } else {
            anyhow::bail!(
                "policy byte export is ambiguous: {} raw-byte matches",
                bytes.len()
            );
        };
        write_export_atomic(path, selected)?;
    }
    match args.format {
        OutputFormat::Json => println!("{}", serde_json::to_string(&report)?),
        OutputFormat::Text => {
            for entry in &report.matches {
                println!(
                    "{} {} {} bytes {}",
                    entry.policy_digest,
                    entry.input_sha256,
                    entry.byte_length,
                    if entry.recorded {
                        "recorded"
                    } else {
                        "unrecorded"
                    }
                );
            }
        }
    }
    Ok(exit_codes::EXIT_SUCCESS)
}
