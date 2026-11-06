//! Every test directory under `crates/*/tests/` must reach a test target.
//!
//! Cargo builds `tests/<name>.rs` and `tests/<dir>/main.rs` as test targets, and nothing else.
//! A directory is only compiled when one of those targets mounts it, through `mod <dir>;` from a
//! sibling `tests/*.rs` or through a `#[path = "..."]` that points into it. Otherwise its
//! `#[test]` functions never run, and nothing reports that. `crates/assay-core/tests/explain_suite/`
//! sat like that for ten months after `explain_suite.rs` was moved to `explain_suite/mod.rs`
//! (commit aeb33a3af): 26 tests, never compiled.
//!
//! This walk fails for a directory that holds `.rs` files, has no `main.rs`, and is mounted by no
//! test target file.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

/// One path component, admitted only if it is an ordinary name (same rule as
/// `retain_callsite_inventory` and `error_enums_non_exhaustive`).
fn safe_component(name: &std::ffi::OsStr) -> Option<String> {
    let s = name.to_str()?;
    let ordinary = !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    ordinary.then(|| s.to_string())
}

fn children(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .filter_map(|entry| {
            let entry = entry.expect("dir entry");
            safe_component(&entry.file_name()).map(|name| dir.join(name))
        })
        .collect();
    out.sort();
    out
}

fn has_rust_file(dir: &Path) -> bool {
    children(dir).iter().any(|path| {
        if path.is_dir() {
            has_rust_file(path)
        } else {
            path.extension().is_some_and(|ext| ext == "rs")
        }
    })
}

/// Files Cargo itself builds as test targets: `tests/*.rs` and `tests/*/main.rs`.
fn target_files(tests_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for tests in tests_dirs {
        for path in children(tests) {
            if path.is_file() && path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            } else if path.join("main.rs").is_file() {
                out.push(path.join("main.rs"));
            }
        }
    }
    out
}

/// Every `#[path = "..."]` value in `text`.
fn path_attributes(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("#[path") {
        rest = &rest[start + "#[path".len()..];
        let Some(open) = rest.find('"') else { break };
        let after = &rest[open + 1..];
        let Some(close) = after.find('"') else { break };
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    out
}

/// `mod <name>;` as a whole statement, so `mod explain_suite_extra;` does not count.
fn declares_module(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        let line = line.strip_prefix("pub ").unwrap_or(line);
        line.strip_prefix("mod ")
            .and_then(|rest| rest.strip_suffix(';'))
            .is_some_and(|declared| declared.trim() == name)
    })
}

fn is_mounted(dir: &Path, targets: &[PathBuf]) -> bool {
    if dir.join("main.rs").is_file() {
        return true;
    }
    let name = dir.file_name().and_then(|n| n.to_str()).expect("dir name");
    let canonical_dir = dir.canonicalize().expect("test dir");
    targets.iter().any(|target| {
        let text = fs::read_to_string(target).expect("test target is UTF-8");
        let same_tests_dir = target.parent() == dir.parent();
        if same_tests_dir && declares_module(&text, name) {
            return true;
        }
        path_attributes(&text).iter().any(|value| {
            target
                .parent()
                .expect("target parent")
                .join(value)
                .canonicalize()
                .is_ok_and(|resolved| resolved.starts_with(&canonical_dir))
        })
    })
}

#[test]
fn every_test_directory_reaches_a_test_target() {
    let root = workspace_root();
    let tests_dirs: Vec<PathBuf> = children(&root.join("crates"))
        .into_iter()
        .map(|krate| krate.join("tests"))
        .filter(|tests| tests.is_dir())
        .collect();
    let targets = target_files(&tests_dirs);

    let mut checked = Vec::new();
    let mut unmounted = Vec::new();
    for tests in &tests_dirs {
        for dir in children(tests) {
            if !dir.is_dir() || !has_rust_file(&dir) {
                continue;
            }
            // Joined with `/` on every platform: `Path::display` uses `\` on Windows, and the
            // assertions below compare against `/`-separated names.
            let shown = dir
                .strip_prefix(&root)
                .unwrap_or(&dir)
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            if !is_mounted(&dir, &targets) {
                unmounted.push(shown.clone());
            }
            checked.push(shown);
        }
    }

    // An empty walk must not pass: the workspace has many mounted test directories.
    assert!(
        checked.len() >= 10,
        "walk found only {} test directories: {checked:?}",
        checked.len()
    );
    assert!(
        checked
            .iter()
            .any(|dir| dir.ends_with("assay-core/tests/explain_suite")),
        "walk must see crates/assay-core/tests/explain_suite: {checked:?}"
    );
    assert!(
        unmounted.is_empty(),
        "test directories that no test target compiles (add main.rs, or mount them with \
         `mod <dir>;` from a tests/*.rs or with #[path]): {unmounted:?}"
    );
}
