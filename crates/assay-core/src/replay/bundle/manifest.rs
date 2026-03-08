use super::{paths, BundleEntry};
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

/// Build a file manifest (path -> FileManifestEntry) from entries. Fail-closed: invalid path -> Error
/// (same policy as writer). Paths must be valid and under files/, outputs/, or cassettes/.
pub fn build_file_manifest(
    entries: &[BundleEntry],
) -> Result<BTreeMap<String, crate::replay::manifest::FileManifestEntry>> {
    let mut out = BTreeMap::new();
    for e in entries {
        let path = paths::validate_entry_path(&e.path)?;
        let hash = Sha256::digest(&e.data);
        out.insert(
            path.clone(),
            crate::replay::manifest::FileManifestEntry {
                sha256: format!("sha256:{}", hex::encode(hash)),
                size: e.data.len() as u64,
                mode: Some(0o644),
                content_type: content_type_hint(Path::new(&path)),
            },
        );
    }
    Ok(out)
}

fn content_type_hint(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_str()?;
    Some(match ext {
        "json" => "application/json".to_string(),
        "jsonl" => "application/x-ndjson".to_string(),
        "xml" => "application/xml".to_string(),
        "yaml" | "yml" => "application/x-yaml".to_string(),
        _ => return None,
    })
}
