//! Every config and policy the documentation and examples show loads in the Assay reader that reads it.
//!
//! An example is what a user copies. The readers refuse keys they do not act on, so an example that
//! drifted from its reader is now an error the user meets on first run. This test takes each YAML
//! block in the user documentation and each example, plugin, pack and demo file, tells its kind from
//! its root keys, and runs it through that kind's reader in the `assay` binary.
//!
//! A block that is deliberately not a whole file (a fragment, an illustration of what does not work,
//! a pre-migration shape) says so on the line before its fence, with a reason:
//! `<!-- docs-corpus: not-a-file: <reason> -->`. Design records (`docs/architecture`, `docs/rfcs`)
//! and archives are history and are not in scope. The enforce policy is read by `assay-mcp-server`;
//! its examples are checked there (`every_enforce_policy_in_the_repository_loads`).

use std::path::{Path, PathBuf};
use std::process::Command;

const MARKER: &str = "<!-- docs-corpus: not-a-file:";

/// Markdown trees in scope, and trees under them that are not.
const DOC_ROOTS: &[&str] = &["docs", "examples", "packaging"];
const DOC_SKIP: &[&str] = &[
    "docs/architecture",
    "docs/rfcs",
    "docs/archive",
    "docs/experiments",
];
/// YAML files in scope.
const FILE_ROOTS: &[&str] = &[
    "examples",
    "packaging",
    "packs/open",
    "crates/assay-evidence/packs",
    "demo/fixtures",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Eval,
    Mcp,
    Trust,
    Sandbox,
    Pack,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn kind_of(doc: &serde_yaml::Value) -> Option<Kind> {
    let map = doc.as_mapping()?;
    let has = |key: &str| map.contains_key(serde_yaml::Value::String(key.to_string()));
    if has("jobs") || has("mandate_kind") || has("caller") {
        return None; // a workflow, a mandate, an enforce policy (read by assay-mcp-server)
    }
    if has("tests") && (has("suite") || has("version") || has("configVersion") || has("model")) {
        return Some(Kind::Eval);
    }
    if has("require_signed") || has("trusted_keys") || has("trusted_key_ids") {
        return Some(Kind::Trust);
    }
    if has("api_version") && (has("fs") || has("net") || has("extends")) {
        return Some(Kind::Sandbox);
    }
    if has("rules") && has("name") && has("version") && has("kind") {
        return Some(Kind::Pack);
    }
    const MCP: &[&str] = &[
        "tools",
        "schemas",
        "runtime_monitor",
        "kill_switch",
        "sequences",
        "aliases",
        "blocklist",
        "discovery",
        "tool_pins",
        "tool_classes",
        "enforcement",
        "limits",
        "constraints",
        "allow",
        "deny",
        "signatures",
    ];
    MCP.iter().any(|key| has(key)).then_some(Kind::Mcp)
}

fn assay() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_assay"));
    command.env("NO_COLOR", "1").env("ASSAY_COLOR", "never");
    command
}

/// Why the reader of `kind` refused `path`, or `None` when it loaded it.
fn refusal(kind: Kind, path: &Path, scratch: &Path) -> Option<String> {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    match kind {
        Kind::Mcp => {
            let out = assay()
                .args(["policy", "validate", "--input"])
                .arg(path)
                .output()
                .expect("run policy validate");
            let stderr = text(&out.stderr);
            let notes: Vec<&str> = stderr
                .lines()
                .filter(|line| line.contains("will refuse this policy"))
                .collect();
            if !out.status.success() {
                Some(stderr.lines().take(3).collect::<Vec<_>>().join(" | "))
            } else if !notes.is_empty() {
                Some(notes.join(" | "))
            } else {
                None
            }
        }
        Kind::Eval => {
            let out = assay()
                .args(["validate", "--format", "json", "--config"])
                .arg(path)
                .current_dir(path.parent().expect("parent"))
                .output()
                .expect("run validate");
            let Ok(report) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
                // No report is not a pass: say what the reader printed instead.
                return Some(format!(
                    "validate wrote no JSON report: {}",
                    text(&out.stderr).trim()
                ));
            };
            let refused: Vec<String> = report["diagnostics"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["code"] == "E_CFG_PARSE")
                // A file the block names (a policy beside it) is not this block's to provide.
                .filter(|d| {
                    !d["message"]
                        .as_str()
                        .is_some_and(|m| m.contains("failed to read"))
                })
                .map(|d| d["message"].as_str().unwrap_or_default().to_string())
                .collect();
            (!refused.is_empty()).then(|| refused.join(" | "))
        }
        Kind::Trust => {
            let tool = scratch.join("unsigned-tool.json");
            std::fs::write(&tool, r#"{"name":"t","inputSchema":{"type":"object"}}"#).expect("tool");
            let out = assay()
                .args(["mcp", "tool", "verify"])
                .arg(&tool)
                .arg("--trust-policy")
                .arg(path)
                .output()
                .expect("run verify");
            let stderr = text(&out.stderr);
            (stderr.contains("trust policy")).then(|| stderr.trim().to_string())
        }
        Kind::Sandbox => {
            let out = assay()
                .args(["sandbox", "--dry-run", "--policy"])
                .arg(path)
                .args(["--", "true"])
                .output()
                .expect("run sandbox");
            let stderr = text(&out.stderr);
            stderr.contains("Failed to load policy").then(|| {
                stderr
                    .lines()
                    .find(|l| l.contains("Failed"))
                    .unwrap_or("")
                    .to_string()
            })
        }
        Kind::Pack => {
            let bundle = repo().join(
                "conformance/privileged-mcp-action-v0/vectors/ok-001-deny-bound-observation.bundle.tar.gz",
            );
            let out = assay()
                .args(["evidence", "lint"])
                .arg(bundle)
                .arg("--pack")
                .arg(path)
                .args(["--format", "json"])
                .output()
                .expect("run lint");
            let stderr = text(&out.stderr);
            stderr
                .contains("Pack loading failed")
                .then(|| stderr.trim().to_string())
        }
    }
}

fn walk(dir: &Path, ext: &[&str], skip: &[PathBuf], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if skip.iter().any(|s| path.starts_with(s)) {
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name.starts_with('.') || name == "target" || name == "node_modules" {
            continue;
        }
        if path.is_dir() {
            walk(&path, ext, skip, out);
        } else if path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| ext.contains(&e))
        {
            out.push(path);
        }
    }
}

/// (where, text) for every YAML block in scope that is not marked as not a file.
fn blocks(root: &Path) -> Vec<(String, String)> {
    let skip: Vec<PathBuf> = DOC_SKIP.iter().map(|s| root.join(s)).collect();
    let mut docs = vec![root.join("README.md")];
    for dir in DOC_ROOTS {
        walk(&root.join(dir), &["md"], &skip, &mut docs);
    }
    let mut found = Vec::new();
    for doc in docs {
        let Ok(text) = std::fs::read_to_string(&doc) else {
            continue;
        };
        let rel = doc.strip_prefix(root).unwrap_or(&doc).display().to_string();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let fence = lines[i].trim();
            if fence == "```yaml" || fence == "```yml" {
                let marked = i > 0 && lines[i - 1].trim_start().starts_with(MARKER);
                let start = i + 1;
                let mut end = start;
                while end < lines.len() && !lines[end].trim_start().starts_with("```") {
                    end += 1;
                }
                if !marked {
                    found.push((
                        format!("{rel}:{start}"),
                        lines[start..end].join("\n") + "\n",
                    ));
                }
                i = end + 1;
            } else {
                i += 1;
            }
        }
    }
    for dir in FILE_ROOTS {
        let mut files = Vec::new();
        walk(&root.join(dir), &["yaml", "yml"], &[], &mut files);
        for file in files {
            if let Ok(text) = std::fs::read_to_string(&file) {
                let rel = file
                    .strip_prefix(root)
                    .unwrap_or(&file)
                    .display()
                    .to_string();
                found.push((rel, text));
            }
        }
    }
    found
}

#[test]
fn every_documented_config_and_policy_loads_in_its_reader() {
    let root = repo();
    let scratch = tempfile::tempdir().expect("scratch");
    let mut checked = std::collections::BTreeMap::<&str, usize>::new();
    let mut failures = Vec::new();
    for (index, (place, text)) in blocks(&root).into_iter().enumerate() {
        let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
            continue; // not one YAML document, so not one file a reader takes
        };
        let Some(kind) = kind_of(&doc) else {
            continue;
        };
        *checked
            .entry(match kind {
                Kind::Eval => "eval config",
                Kind::Mcp => "policy",
                Kind::Trust => "trust policy",
                Kind::Sandbox => "sandbox policy",
                Kind::Pack => "pack",
            })
            .or_default() += 1;
        let path = if place.contains(".md:") {
            let path = scratch.path().join(format!("block-{index}.yaml"));
            std::fs::write(&path, &text).expect("write block");
            path
        } else {
            root.join(&place)
        };
        if let Some(why) = refusal(kind, &path, scratch.path()) {
            failures.push(format!("{place} ({kind:?}): {why}"));
        }
    }
    assert!(
        checked.get("eval config").copied().unwrap_or(0) > 10
            && checked.get("policy").copied().unwrap_or(0) > 10,
        "the corpus did not find the documented configs: {checked:?}"
    );
    assert!(
        failures.is_empty(),
        "{} documented config(s) or policy(ies) do not load in their reader. Fix the example, or mark a \
         block that is deliberately not a whole file with `{MARKER} <reason> -->` on the line before \
         its fence.\n\n{}\n\nchecked: {checked:?}",
        failures.len(),
        failures.join("\n")
    );
}
