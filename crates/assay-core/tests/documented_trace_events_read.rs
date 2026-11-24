//! Recognizable event lines in selected examples deserialize as the `TraceEvent` replay reads.
//!
//! Checked: each line in regular `examples/**/*.jsonl` files, and each line inside a fenced `json` or `jsonl`
//! block under `docs/concepts/`, that parses as a JSON object whose `type` is a string other than
//! `assay.trace`. Not checked: lines that are not valid JSON objects, lines without such a `type`,
//! episode membership, and any other docs. A file with no recognizable event line passes. Replay
//! refuses an event it cannot read, so a checked line it refuses would teach a trace that stops
//! evaluation.

use assay_core::trace::schema::TraceEvent;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(dir: &Path, ext: &str, out: &mut Vec<PathBuf>) {
    for entry in walkdir::WalkDir::new(dir).follow_root_links(false) {
        let entry = entry.expect("read example directory");
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|e| e == ext) {
            out.push(entry.into_path());
        }
    }
}

/// Lines inside fenced `jsonl`/`json` blocks of a Markdown file.
fn fenced_lines(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let fence = line.trim_start();
        if let Some(info) = fence.strip_prefix("```") {
            inside = !inside && matches!(info.trim(), "jsonl" | "json");
            continue;
        }
        if inside {
            out.push(line.to_string());
        }
    }
    out
}

fn typed_line_problem(line: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(line.trim()).ok()?;
    let kind = value.get("type").and_then(serde_json::Value::as_str)?;
    if kind == "assay.trace" {
        return None;
    }
    serde_json::from_value::<TraceEvent>(value)
        .err()
        .map(|e| e.to_string())
}

#[test]
fn recognizable_event_lines_in_selected_examples_are_readable() {
    let root = root();
    let mut problems = Vec::new();

    let mut traces = Vec::new();
    files(&root.join("examples"), "jsonl", &mut traces);
    for path in traces {
        let text = std::fs::read_to_string(&path).expect("read trace");
        for (i, line) in text.lines().enumerate() {
            if let Some(problem) = typed_line_problem(line) {
                problems.push(format!("{}:{}: {problem}", path.display(), i + 1));
            }
        }
    }

    let mut pages = Vec::new();
    files(&root.join("docs/concepts"), "md", &mut pages);
    for path in pages {
        let text = std::fs::read_to_string(&path).expect("read page");
        for line in fenced_lines(&text) {
            if let Some(problem) = typed_line_problem(&line) {
                problems.push(format!("{}: {problem}: {line}", path.display()));
            }
        }
    }

    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[cfg(unix)]
#[test]
fn example_inventory_counts_regular_files_once() {
    let root = tempfile::tempdir().unwrap();
    let example = root.path().join("one.jsonl");
    std::fs::write(&example, "").unwrap();
    std::os::unix::fs::symlink(&example, root.path().join("alias.jsonl")).unwrap();
    let mut found = Vec::new();
    files(root.path(), "jsonl", &mut found);
    assert_eq!(found, [example]);
}
