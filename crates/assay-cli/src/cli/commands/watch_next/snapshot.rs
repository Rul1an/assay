use std::collections::{BTreeMap, BTreeSet};
use std::hash::{DefaultHasher, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::MAX_SNAPSHOT_HASH_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FileSnapshot {
    pub(super) exists: bool,
    pub(super) len: Option<u64>,
    pub(super) modified: Option<SystemTime>,
    pub(super) content_hash: Option<u64>,
}

pub(super) fn snapshot_paths(paths: &[PathBuf]) -> BTreeMap<PathBuf, FileSnapshot> {
    let mut out = BTreeMap::new();
    for path in paths {
        let snapshot = match std::fs::metadata(path) {
            Ok(meta) => {
                let len = meta.len();
                FileSnapshot {
                    exists: true,
                    len: Some(len),
                    modified: meta.modified().ok(),
                    content_hash: snapshot_content_hash(path, len),
                }
            }
            Err(_) => FileSnapshot {
                exists: false,
                len: None,
                modified: None,
                content_hash: None,
            },
        };
        out.insert(path.clone(), snapshot);
    }
    out
}

fn snapshot_content_hash(path: &Path, len: u64) -> Option<u64> {
    if len > MAX_SNAPSHOT_HASH_BYTES {
        return None;
    }

    let file = std::fs::File::open(path).ok()?;
    let mut reader = file.take(MAX_SNAPSHOT_HASH_BYTES + 1);
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_SNAPSHOT_HASH_BYTES {
        return None;
    }

    let mut hasher = DefaultHasher::new();
    hasher.write(&bytes);
    Some(hasher.finish())
}

pub(super) fn diff_paths(
    prev: &BTreeMap<PathBuf, FileSnapshot>,
    curr: &BTreeMap<PathBuf, FileSnapshot>,
) -> Vec<PathBuf> {
    let mut changed = Vec::new();

    let all_paths: BTreeSet<PathBuf> = prev.keys().chain(curr.keys()).cloned().collect();
    for path in all_paths {
        let prev_state = prev.get(&path);
        let curr_state = curr.get(&path);
        if prev_state != curr_state {
            changed.push(path);
        }
    }

    changed
}

pub(super) fn coalesce_changed_paths(changed: &mut Vec<PathBuf>) {
    changed.sort();
    changed.dedup();
}
