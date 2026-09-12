//! Production retain paths must choose a ceiling; `BundleReader::open` is not that choice.
//!
//! `open` / `open_unverified` inherit `VerifyLimits::default().max_events_bytes` (500 MiB) and
//! keep the whole decompressed `events.ndjson`. A new untrusted or long-lived caller that picks
//! those constructors re-adopts full residency without saying so. Comments cannot carry that
//! rule: this walk fails when a production `src/` call site writes one of those constructors.
//!
//! Tests, rustdoc, and `#[cfg(test)]` modules are out of scope. They are not ingest entrypoints.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// One path component, admitted only if it is an ordinary name.
///
/// Same rule `error_enums_non_exhaustive` uses: `read_dir` entries are untrusted input to a
/// static analyser, and a symlink under `crates/` must not send this walk outside the workspace.
fn safe_component(name: &std::ffi::OsStr) -> Option<String> {
    let s = name.to_str()?;
    let ordinary = !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ordinary.then(|| s.to_string())
}

fn resolves_inside(root: &Path, p: &Path) -> bool {
    p.canonicalize().is_ok_and(|r| r.starts_with(root))
}

fn walk_src_rs(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let entry = entry.expect("dir entry");
        let Some(name) = safe_component(&entry.file_name()) else {
            continue;
        };
        let path = dir.join(&name);
        if !resolves_inside(root, &path) {
            continue;
        }
        if path.is_dir() {
            walk_src_rs(root, &path, out);
            continue;
        }
        if name.ends_with(".rs") && name != "tests.rs" {
            out.push(path);
        }
    }
}

fn crate_src_files(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let crates_dir = root.join("crates");
    for entry in fs::read_dir(&crates_dir).expect("crates/ is readable") {
        let Some(name) = safe_component(&entry.expect("dir entry").file_name()) else {
            continue;
        };
        let src = crates_dir.join(&name).join("src");
        if src.is_dir() && resolves_inside(root, &src) {
            walk_src_rs(root, &src, &mut out);
        }
    }
    out
}

/// `BundleReader::open(` / `open_unverified(` — not the `*_with_limits` siblings.
fn is_default_open_call(line: &str) -> bool {
    let Some(idx) = line.find("BundleReader::open") else {
        return false;
    };
    let rest = &line[idx + "BundleReader::open".len()..];
    rest.starts_with('(') || rest.starts_with("_unverified(")
}

fn is_comment_or_doc(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("//") || trimmed.starts_with("///") || trimmed.starts_with("//!")
}

fn is_mod_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    let after_vis = trimmed
        .strip_prefix("pub(crate) ")
        .or_else(|| trimmed.strip_prefix("pub(super) "))
        .or_else(|| trimmed.strip_prefix("pub "))
        .unwrap_or(trimmed);
    after_vis.starts_with("mod ")
}

/// Skip bodies of `#[cfg(test)] mod ... { ... }`. Those are not ingest entrypoints.
fn default_open_hits(source: &str) -> Vec<(usize, String)> {
    let mut hits = Vec::new();
    let mut in_test_mod = false;
    let mut test_depth = 0usize;
    let mut pending_test_mod = false;

    for (idx, line) in source.lines().enumerate() {
        if !in_test_mod && line.contains("#[cfg(test)]") {
            pending_test_mod = true;
        }

        if pending_test_mod && is_mod_start(line) {
            in_test_mod = true;
            pending_test_mod = false;
            test_depth = 0;
        }

        if in_test_mod {
            for c in line.chars() {
                match c {
                    '{' => test_depth += 1,
                    '}' => {
                        test_depth = test_depth.saturating_sub(1);
                        if test_depth == 0 {
                            in_test_mod = false;
                        }
                    }
                    _ => {}
                }
            }
            continue;
        }

        if pending_test_mod && !line.trim().is_empty() && !line.contains("#[cfg(test)]") {
            // `#[cfg(test)]` on a single item, not a module. Still skip this line if it is
            // the item itself; otherwise the attribute did not introduce a skipped region.
            pending_test_mod = false;
        }

        if is_comment_or_doc(line) {
            continue;
        }
        if is_default_open_call(line) {
            hits.push((idx + 1, line.trim().to_string()));
        }
    }
    hits
}

#[test]
fn production_src_does_not_call_default_bundle_reader_open() {
    let root = workspace_root();
    let mut offenders = Vec::new();

    for path in crate_src_files(&root) {
        let source = fs::read_to_string(&path).expect("read rust source");
        for (line, text) in default_open_hits(&source) {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            offenders.push(format!("{rel}:{line}: {text}"));
        }
    }

    assert!(
        offenders.is_empty(),
        "production src called BundleReader::open or open_unverified, which inherit the 500 MiB \
         default residency ceiling. Untrusted or long-lived callers that only need a verify or \
         manifest answer must use verify_bundle / BundleInfo::peek_and_bound_events; callers that \
         need events must pass an explicit tighter limit via open_with_limits. Offenders:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn public_reader_docs_do_not_claim_open_is_cheap_relative_to_disk() {
    let root = workspace_root();
    let reader = root.join("crates/assay-evidence/src/bundle/reader.rs");
    let source = fs::read_to_string(&reader).expect("reader.rs");

    let banned = [
        "typically <100MB",
        "typically < 100MB",
        "acceptable because",
        "Faster than `BundleReader::open()`",
        "Faster than BundleReader::open()",
    ];
    let mut hits = Vec::new();
    for phrase in banned {
        if source.contains(phrase) {
            hits.push(phrase);
        }
    }

    assert!(
        hits.is_empty(),
        "public reader docs claimed opening a bundle is cheap relative to on-disk size ({hits:?})"
    );
}
