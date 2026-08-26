use anyhow::{bail, Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_MANIFEST_BYTES: usize = 1024 * 1024;

pub fn read_bounded_bytes(path: &Path) -> Result<Vec<u8>> {
    let file = File::open(path).with_context(|| format!("reading {}", path.display()))?;
    let mut bytes = Vec::new();
    file.take((MAX_MANIFEST_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading {}", path.display()))?;
    if bytes.len() > MAX_MANIFEST_BYTES {
        bail!(
            "{} exceeds the {}-byte manifest limit",
            path.display(),
            MAX_MANIFEST_BYTES
        );
    }
    Ok(bytes)
}

pub fn write_json_create_new(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_MANIFEST_BYTES {
        bail!(
            "serialized output for {} exceeds the {}-byte manifest limit",
            path.display(),
            MAX_MANIFEST_BYTES
        );
    }

    let parent = usable_parent(path);
    let mut temp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temporary output beside {}", path.display()))?;
    temp.write_all(&bytes)
        .with_context(|| format!("writing temporary output for {}", path.display()))?;
    temp.flush()
        .with_context(|| format!("flushing temporary output for {}", path.display()))?;
    temp.persist_noclobber(path).map_err(|error| {
        anyhow::anyhow!(
            "creating {} without overwriting an existing file: {}",
            path.display(),
            error.error
        )
    })?;
    Ok(())
}

fn usable_parent(path: &Path) -> PathBuf {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_exactly_at_the_limit_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("exact.json");
        let empty = serde_json::to_vec_pretty(&json!({"payload": ""})).unwrap();
        let payload_len = MAX_MANIFEST_BYTES - empty.len() - 1;
        let value = json!({"payload": "x".repeat(payload_len)});

        write_json_create_new(&out, &value).expect("the inclusive limit must be accepted");
        assert_eq!(
            std::fs::metadata(out).unwrap().len(),
            MAX_MANIFEST_BYTES as u64
        );
    }

    #[test]
    fn oversized_json_is_rejected_without_output_or_temp_residue() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("too-large.json");
        let value = json!({"payload": "x".repeat(MAX_MANIFEST_BYTES)});

        let error = write_json_create_new(&out, &value)
            .expect_err("an output above the shared manifest limit must fail");
        assert!(error.to_string().contains("manifest limit"));
        assert!(!out.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}
