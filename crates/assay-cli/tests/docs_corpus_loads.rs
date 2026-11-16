//! Every config and policy the documentation and examples show loads in the Assay reader that reads it.
//!
//! An example is what a user copies. The readers refuse keys they do not act on, so an example that
//! drifted from its reader is now an error the user meets on first run. This test takes each YAML
//! block in the user documentation and each example, plugin, pack and demo file, tells its kind from
//! its root keys (a list of sequence rules from its items), and runs it through that kind's reader in
//! the `assay` binary.
//!
//! A block that shows only `tests:` (no `version`, `configVersion`, `suite` or `model`) is the tests
//! of a config, and is checked under a minimal header. A block with any of those keys is a whole
//! file and must load as written.
//!
//! A block that is deliberately not a whole file (a fragment, an illustration of what does not work,
//! a pre-migration shape) says so on the line before its fence, with the marker the sequence-rule
//! docs test uses: `<!-- docs-example: not-a-config (<reason>) -->`. Every exemption is pinned in
//! `EXEMPTIONS`, so adding one is a reviewed decision rather than a way around this test. Design
//! records (`docs/architecture`, `docs/rfcs`) and archives are history and are not in scope; the
//! specifications beside them (`docs/architecture/SPEC-*.md`) are normative, and are.
//!
//! The enforce policy is read by `assay-mcp-server`; its examples are checked there
//! (`every_enforce_policy_in_the_repository_loads`).
//!
//! A reader whose refusal this test cannot see would pass every example, so each reader is also
//! given one file it loads and one it refuses, and must be read as doing so.

use std::path::{Path, PathBuf};
use std::process::Command;

const MARKER: &str = "<!-- docs-example: not-a-config";

/// Every marked block in scope, as (page, the marker's reason), sorted.
const EXEMPTIONS: &[(&str, &str)] = &[
    (
        "docs/architecture/SPEC-Pack-Engine-v1.md",
        "a schema that gives each field's type in place of a value",
    ),
    (
        "docs/concepts/fail-safe.md",
        "the tests are elided to show where settings.on_error goes",
    ),
    (
        "docs/concepts/sandbox.md",
        "an allow and a deny that Landlock cannot enforce together",
    ),
    ("docs/guides/migration.md", "the expected block is elided"),
    (
        "docs/reference/config/sequences.md",
        "a v0 format that no version reads",
    ),
    ("docs/reference/config/sequences.md", "shows test ids only"),
    (
        "docs/reference/policies.md",
        "a key that assay mcp wrap refuses",
    ),
];

/// Markdown trees in scope, and trees under them that are not.
const DOC_ROOTS: &[&str] = &["docs", "examples", "packaging"];
const DOC_SKIP: &[&str] = &[
    "docs/architecture",
    "docs/rfcs",
    "docs/archive",
    "docs/experiments",
];
/// The specifications in a skipped tree, which are in scope: files named `SPEC-*.md` under it.
const SPEC_DIR: &str = "docs/architecture";
const SPEC_PREFIX: &str = "SPEC-";
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
    /// A list of sequence rules: the file a `sequence_valid` test's `policy:` names, and the file
    /// `assay_check_sequence` reads. It is checked through an eval config that names it, as a test
    /// reads it.
    Sequence,
    /// A list of trace assertions: the `assertions:` of a test, checked under a test.
    Assertions,
}

/// The `type` of each `TraceAssertion` (crates/assay-core/src/agent_assertions/model.rs). A list
/// with any of them is a test's `assertions:`; any other list of `type:` items is sequence rules.
const ASSERTION_TYPES: &[&str] = &[
    "trace_must_call_tool",
    "trace_must_not_call_tool",
    "trace_tool_sequence",
    "trace_max_steps",
    "args_valid",
    "sequence_valid",
    "tool_blocklist",
];

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn kind_of(doc: &serde_yaml::Value) -> Option<Kind> {
    if let Some(items) = doc.as_sequence() {
        let is_rule = |item: &serde_yaml::Value| {
            item.as_mapping()
                .is_some_and(|rule| rule.contains_key(serde_yaml::Value::String("type".into())))
        };
        if items.is_empty() || !items.iter().all(is_rule) {
            return None;
        }
        let is_assertion = |item: &serde_yaml::Value| {
            item.get("type")
                .and_then(|kind| kind.as_str())
                .is_some_and(|kind| ASSERTION_TYPES.contains(&kind))
        };
        return Some(if items.iter().any(is_assertion) {
            Kind::Assertions
        } else {
            Kind::Sequence
        });
    }
    let map = doc.as_mapping()?;
    let has = |key: &str| map.contains_key(serde_yaml::Value::String(key.to_string()));
    if has("jobs") || has("mandate_kind") || has("caller") {
        return None; // a workflow, a mandate, an enforce policy (read by assay-mcp-server)
    }
    if has("tests") {
        return Some(Kind::Eval);
    }
    if has("require_signed") || has("trusted_keys") || has("trusted_key_ids") {
        return Some(Kind::Trust);
    }
    // `api_version` defaults, so a sandbox policy may have only `fs`, `net` or `extends`.
    if has("fs") || has("net") || has("extends") {
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

/// The `assay` binary built from this tree.
fn this_tree() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_assay"))
}

fn assay(bin: &Path) -> Command {
    let mut command = Command::new(bin);
    command.env("NO_COLOR", "1").env("ASSAY_COLOR", "never");
    command
}

/// The `E_CFG_PARSE` diagnostics `assay validate` reports for the eval config at `path`, or what
/// it printed instead of a report. With `skip_unread_files`, a file the config names that does not
/// exist is not counted.
fn eval_refusal(bin: &Path, path: &Path, skip_unread_files: bool) -> Option<String> {
    let out = assay(bin)
        .args(["validate", "--format", "json", "--config"])
        .arg(path)
        .current_dir(path.parent().expect("parent"))
        .output()
        .expect("run validate");
    let Ok(report) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        // No report is not a pass: say what the reader printed instead.
        return Some(format!(
            "validate wrote no JSON report: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    };
    let refused: Vec<String> = report["diagnostics"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| d["code"] == "E_CFG_PARSE")
        .map(|d| d["message"].as_str().unwrap_or_default().to_string())
        .filter(|message| !(skip_unread_files && message.contains("failed to read")))
        .collect();
    (!refused.is_empty()).then(|| refused.join(" | "))
}

/// Why the reader of `kind` in the binary `bin` refused `path`, or `None` when it loaded it.
fn refusal(bin: &Path, kind: Kind, path: &Path, scratch: &Path) -> Option<String> {
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    match kind {
        Kind::Mcp => {
            let out = assay(bin)
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
        // A file the block names (a policy beside it) is not this block's to provide.
        Kind::Eval => eval_refusal(bin, path, true),
        Kind::Sequence => {
            let config = scratch.join(format!(
                "names-{}",
                path.file_name().expect("file name").to_string_lossy()
            ));
            let policy = serde_json::to_string(&path.display().to_string()).expect("quote path");
            std::fs::write(
                &config,
                format!(
                    "version: 1\nsuite: docs-snippet\nmodel: dummy\ntests:\n  - id: sequence\n    \
                     input:\n      prompt: x\n    expected:\n      type: sequence_valid\n      \
                     policy: {policy}\n"
                ),
            )
            .expect("write the config that names the sequence file");
            eval_refusal(bin, &config, false)
        }
        Kind::Assertions => {
            let config = scratch.join(format!(
                "under-a-test-{}",
                path.file_name().expect("file name").to_string_lossy()
            ));
            let assertions: String = read(path)
                .lines()
                .map(|line| format!("      {line}\n"))
                .collect();
            std::fs::write(
                &config,
                format!(
                    "version: 1\nsuite: docs-snippet\nmodel: dummy\ntests:\n  - id: assertions\n    \
                     input:\n      prompt: x\n    assertions:\n{assertions}"
                ),
            )
            .expect("write the config that holds the assertions");
            eval_refusal(bin, &config, true)
        }
        Kind::Trust => {
            let tool = scratch.join("unsigned-tool.json");
            std::fs::write(&tool, r#"{"name":"t","inputSchema":{"type":"object"}}"#).expect("tool");
            let out = assay(bin)
                .args(["mcp", "tool", "verify"])
                .arg(&tool)
                .arg("--trust-policy")
                .arg(path)
                .output()
                .expect("run verify");
            // The three ways TrustPolicy::from_file fails. A valid policy that rejects this
            // unsigned tool fails too, but on the tool, not the policy.
            const LOAD_ERRORS: &[&str] = &[
                "failed to read trust policy",
                "failed to parse trust policy",
                "refuses this trust policy",
            ];
            let stderr = text(&out.stderr);
            LOAD_ERRORS
                .iter()
                .any(|error| stderr.contains(error))
                .then(|| stderr.trim().to_string())
        }
        Kind::Sandbox => {
            let out = assay(bin)
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
            let out = assay(bin)
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

/// Every file under `dir` with one of `ext`, in sorted order, leaving out `skip`, hidden entries,
/// `target` and `node_modules`. A tree in scope that cannot be read fails the test: a file left out
/// is not a file that loaded.
fn walk(dir: &Path, ext: &[&str], skip: &[PathBuf], out: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display()));
    let mut entries: Vec<PathBuf> = entries
        .map(|entry| {
            entry
                .unwrap_or_else(|e| panic!("read an entry of {}: {e}", dir.display()))
                .path()
        })
        .collect();
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

/// The reason of a not-a-config marker on `line`, if it is one.
fn marker_reason(line: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(MARKER)?.trim();
    let inner = rest.strip_suffix("-->")?.trim();
    Some(
        inner
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .unwrap_or(inner)
            .to_string(),
    )
}

/// `path` under `root`, with `/` between its parts on every platform, as EXEMPTIONS spells it.
fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or_else(|_| panic!("{} is not under {}", path.display(), root.display()))
        .components()
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

/// The text of a file in scope; one that cannot be read fails the test.
fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Pairs of strings: (where, text) for a block or file, or (page, reason) for a marked block.
type Pairs = Vec<(String, String)>;

/// (where, text) for every YAML block in scope that is not marked as not a file, and the
/// (page, reason) of every marked one.
fn blocks(root: &Path) -> (Pairs, Pairs) {
    let skip: Vec<PathBuf> = DOC_SKIP.iter().map(|s| root.join(s)).collect();
    let mut docs = vec![root.join("README.md")];
    for dir in DOC_ROOTS {
        walk(&root.join(dir), &["md"], &skip, &mut docs);
    }
    let mut specs = Vec::new();
    walk(&root.join(SPEC_DIR), &["md"], &[], &mut specs);
    docs.extend(specs.into_iter().filter(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(SPEC_PREFIX))
    }));
    let mut found = Vec::new();
    let mut exempt = Vec::new();
    for doc in docs {
        let text = read(&doc);
        let rel = relative(root, &doc);
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let fence = lines[i].trim();
            if fence == "```yaml" || fence == "```yml" {
                let marked = lines[..i]
                    .iter()
                    .rev()
                    .find(|l| !l.trim().is_empty())
                    .and_then(|l| marker_reason(l));
                let start = i + 1;
                let mut end = start;
                while end < lines.len() && !lines[end].trim_start().starts_with("```") {
                    end += 1;
                }
                match marked {
                    Some(reason) => exempt.push((rel.clone(), reason)),
                    None => found.push((
                        format!("{rel}:{start}"),
                        lines[start..end].join("\n") + "\n",
                    )),
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
            found.push((relative(root, &file), read(&file)));
        }
    }
    (found, exempt)
}

/// A `tests:`-only block under the smallest header that makes it a config; anything else as written.
fn with_header(kind: Kind, doc: &serde_yaml::Value, text: &str) -> String {
    let header_keys = ["version", "configVersion", "suite", "model"];
    let has_header = doc.as_mapping().is_some_and(|map| {
        header_keys
            .iter()
            .any(|key| map.contains_key(serde_yaml::Value::String((*key).to_string())))
    });
    if kind == Kind::Eval && !has_header {
        format!("version: 1\nsuite: docs-snippet\nmodel: dummy\n{text}")
    } else {
        text.to_string()
    }
}

#[test]
fn each_reader_is_read_as_loading_a_working_file_and_refusing_a_broken_one() {
    const EVAL: &str = "version: 1\nsuite: s\nmodel: dummy\ntests:\n  - id: a\n    input:\n      \
                        prompt: x\n    expected:\n      type: must_contain\n      must_contain: [x]\n";
    const POLICY: &str = "version: \"2.0\"\ntools:\n  allow: [read_file]\n";
    let pack = std::fs::read_to_string(repo().join("packs/open/cicd-starter/pack.yaml"))
        .expect("read the cicd-starter pack");
    // (kind, a file its reader loads, and the same shape broken so that it refuses it)
    let cases = [
        (
            Kind::Eval,
            EVAL.to_string(),
            format!("{EVAL}output:\n  format: json\n"),
        ),
        (
            Kind::Mcp,
            POLICY.to_string(),
            format!("{POLICY}signatures:\n  check_descriptions: true\n"),
        ),
        (
            Kind::Trust,
            "require_signed: false\ntrusted_keys: []\n".to_string(),
            "require_signed: true\ntrusted_key_ids: [abc]\n".to_string(),
        ),
        (
            Kind::Sandbox,
            "api_version: 1\nfs:\n  allow:\n    - /tmp\n".to_string(),
            "api_version: 1\nfs:\n  bogus: 1\n".to_string(),
        ),
        (Kind::Pack, pack.clone(), format!("{pack}bogus_key: 1\n")),
        (
            Kind::Sequence,
            "- type: before\n  first: verify_identity\n  then: delete_customer\n".to_string(),
            "- type: before\n  then: delete_customer\n".to_string(),
        ),
        (
            Kind::Assertions,
            "- type: trace_max_steps\n  max: 8\n".to_string(),
            "- type: trace_max_steps\n  max: 8\n  min: 1\n".to_string(),
        ),
    ];
    let scratch = tempfile::tempdir().expect("scratch");
    for (kind, loads, refused) in cases {
        for (name, text, want_refusal) in [("loads", &loads, false), ("refused", &refused, true)] {
            let doc: serde_yaml::Value = serde_yaml::from_str(text).expect("control is YAML");
            assert_eq!(
                kind_of(&doc),
                Some(kind),
                "{kind:?} {name} control is read as its kind"
            );
            let path = scratch.path().join(format!("{kind:?}-{name}.yaml"));
            std::fs::write(&path, text).expect("write control");
            let why = refusal(this_tree(), kind, &path, scratch.path());
            assert_eq!(
                why.is_some(),
                want_refusal,
                "{kind:?} {name} control: refusal() returned {why:?}"
            );
        }
    }
}

#[test]
fn every_documented_config_and_policy_loads_in_its_reader() {
    let root = repo();
    let scratch = tempfile::tempdir().expect("scratch");
    let mut checked = std::collections::BTreeMap::<&str, usize>::new();
    let mut failures = Vec::new();
    let (blocks, mut exempt) = blocks(&root);
    exempt.sort();
    let pinned: Vec<(String, String)> = EXEMPTIONS
        .iter()
        .map(|(page, why)| (page.to_string(), why.to_string()))
        .collect();
    assert_eq!(
        exempt, pinned,
        "the not-a-config markers in the docs differ from EXEMPTIONS; pin a new exemption there"
    );
    for (index, (place, text)) in blocks.into_iter().enumerate() {
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
                Kind::Sequence => "sequence rules",
                Kind::Assertions => "assertions",
            })
            .or_default() += 1;
        let path = if place.contains(".md:") {
            let path = scratch.path().join(format!("block-{index}.yaml"));
            std::fs::write(&path, with_header(kind, &doc, &text)).expect("write block");
            path
        } else {
            root.join(&place)
        };
        if let Some(why) = refusal(this_tree(), kind, &path, scratch.path()) {
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
         block that is deliberately not a whole file with `{MARKER} (<reason>) -->` on the line before \
         its fence and pin it in EXEMPTIONS.\n\n{}\n\nchecked: {checked:?}",
        failures.len(),
        failures.join("\n")
    );
}

// ---------------------------------------------------------------------------------------------
// The last release and this tree, over the same documents (ADR-052, "What stops the work").
// ---------------------------------------------------------------------------------------------

/// The key the comparison plants. No reader takes it.
const PLANTED: &str = "planted_by_the_reader_comparison";

/// What a reader said about one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Verdict {
    Loaded,
    Refused,
}

impl Verdict {
    fn word(self) -> &'static str {
        match self {
            Verdict::Loaded => "loaded",
            Verdict::Refused => "refused",
        }
    }
}

/// One text to give to both binaries.
struct Item {
    kind: Kind,
    /// The document it comes from: a file, or a page and the line its block starts on.
    place: String,
    /// Where the unknown key was planted, as a dotted path with `*` for a list index and the
    /// empty string for the root mapping; `None` for the document as written.
    planted: Option<String>,
    text: String,
}

/// Every mapping in `node`, as the steps that lead to it.
fn mappings(
    node: &serde_yaml::Value,
    at: &mut Vec<serde_yaml::Value>,
    out: &mut Vec<Vec<serde_yaml::Value>>,
) {
    match node {
        serde_yaml::Value::Mapping(entries) => {
            out.push(at.clone());
            for (key, child) in entries {
                at.push(key.clone());
                mappings(child, at, out);
                at.pop();
            }
        }
        serde_yaml::Value::Sequence(items) => {
            for (index, child) in items.iter().enumerate() {
                at.push(serde_yaml::Value::Number(index.into()));
                mappings(child, at, out);
                at.pop();
            }
        }
        _ => {}
    }
}

fn mapping_at<'a>(
    root: &'a mut serde_yaml::Value,
    address: &[serde_yaml::Value],
) -> &'a mut serde_yaml::Mapping {
    let mut node = root;
    for step in address {
        node = match node {
            serde_yaml::Value::Mapping(entries) => entries.get_mut(step).expect("the key is there"),
            serde_yaml::Value::Sequence(items) => {
                &mut items[step.as_u64().expect("an index") as usize]
            }
            _ => panic!("the address leads through a scalar"),
        };
    }
    node.as_mapping_mut().expect("the address names a mapping")
}

/// An address as a dotted path: each key as written, `*` for each list index.
fn dotted(address: &[serde_yaml::Value]) -> String {
    address
        .iter()
        .map(|step| match step {
            serde_yaml::Value::String(key) => key.clone(),
            serde_yaml::Value::Number(_) => "*".to_string(),
            other => serde_yaml::to_string(other)
                .unwrap_or_default()
                .trim()
                .to_string(),
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// The corpus of `every_documented_config_and_policy_loads_in_its_reader`, each document as
/// written and once more for every mapping in it, with an unknown key planted there.
fn comparison_items(root: &Path) -> Vec<Item> {
    let (blocks, _) = blocks(root);
    let mut items = Vec::new();
    for (place, text) in blocks {
        let Ok(doc) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
            continue;
        };
        let Some(kind) = kind_of(&doc) else {
            continue;
        };
        let text = if place.contains(".md:") {
            with_header(kind, &doc, &text)
        } else {
            text
        };
        let doc: serde_yaml::Value = serde_yaml::from_str(&text).expect("the document is YAML");
        let mut addresses = Vec::new();
        mappings(&doc, &mut Vec::new(), &mut addresses);
        for address in &addresses {
            let mut planted = doc.clone();
            mapping_at(&mut planted, address).insert(
                serde_yaml::Value::String(PLANTED.to_string()),
                serde_yaml::Value::from(1),
            );
            items.push(Item {
                kind,
                place: place.clone(),
                planted: Some(dotted(address)),
                text: serde_yaml::to_string(&planted).expect("the planted document serializes"),
            });
        }
        items.push(Item {
            kind,
            place,
            planted: None,
            text,
        });
    }
    items
}

/// The verdict of each binary on each item, in the order of `items`.
fn verdicts(bins: [&Path; 2], items: &[Item]) -> Vec<[Verdict; 2]> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(vec![None; items.len()]);
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                let scratch = tempfile::tempdir().expect("scratch");
                loop {
                    let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(item) = items.get(index) else {
                        break;
                    };
                    let path = scratch.path().join(format!("item-{index}.yaml"));
                    std::fs::write(&path, &item.text).expect("write the item");
                    let read = |bin: &Path| match refusal(bin, item.kind, &path, scratch.path()) {
                        Some(_) => Verdict::Refused,
                        None => Verdict::Loaded,
                    };
                    let pair = [read(bins[0]), read(bins[1])];
                    results.lock().expect("results")[index] = Some(pair);
                }
            });
        }
    });
    results
        .into_inner()
        .expect("results")
        .into_iter()
        .map(|pair| pair.expect("every item was read"))
        .collect()
}

/// One difference between the two binaries.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Difference {
    kind: String,
    /// The document, with the line of a block.
    place: String,
    /// `(root)`, a dotted path, or `as-written`.
    at: String,
    before: Verdict,
    now: Verdict,
}

/// One line of `expected_changes.tsv`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Rule {
    line: usize,
    kind: String,
    at: String,
    before: Verdict,
    now: Verdict,
    /// For `as-written`: the page or file, without a line number.
    page: Option<String>,
}

impl Rule {
    fn takes(&self, difference: &Difference) -> bool {
        let page = difference
            .place
            .rsplit_once(".md:")
            .map_or(difference.place.clone(), |(page, _)| format!("{page}.md"));
        self.kind == difference.kind
            && self.at == difference.at
            && self.before == difference.before
            && self.now == difference.now
            && self.page.as_ref().is_none_or(|listed| *listed == page)
    }
}

const AS_WRITTEN: &str = "as-written";
const ROOT: &str = "(root)";
const KINDS: &[&str] = &[
    "Eval",
    "Mcp",
    "Trust",
    "Sandbox",
    "Pack",
    "Sequence",
    "Assertions",
];

/// The release the rules hold against, and the rules.
fn expected_changes(text: &str) -> Result<(String, Vec<Rule>), String> {
    let verdict = |word: &str, line: usize| match word {
        "loaded" => Ok(Verdict::Loaded),
        "refused" => Ok(Verdict::Refused),
        other => Err(format!(
            "line {line}: `{other}` is not `loaded` or `refused`"
        )),
    };
    let mut against = None;
    let mut rules = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        if raw.trim().is_empty() || raw.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = raw.split('\t').collect();
        if fields[0] == "against" {
            if fields.len() != 2 || against.is_some() {
                return Err(format!(
                    "line {line}: expected one `against<TAB>version` line"
                ));
            }
            against = Some(fields[1].to_string());
            continue;
        }
        if !(4..=5).contains(&fields.len()) {
            return Err(format!("line {line}: expected 4 or 5 tab-separated fields"));
        }
        if !KINDS.contains(&fields[0]) {
            return Err(format!("line {line}: `{}` is not a kind", fields[0]));
        }
        let page = fields.get(4).map(|page| (*page).to_string());
        if (fields[1] == AS_WRITTEN) != page.is_some() {
            return Err(format!(
                "line {line}: `{AS_WRITTEN}` takes the page or file as a fifth field, and nothing else does"
            ));
        }
        let rule = Rule {
            line,
            kind: fields[0].to_string(),
            at: fields[1].to_string(),
            before: verdict(fields[2], line)?,
            now: verdict(fields[3], line)?,
            page,
        };
        if rule.before == rule.now {
            return Err(format!(
                "line {line}: the two verdicts are the same, so this is not a change"
            ));
        }
        if rules.iter().any(|other: &Rule| {
            (&other.kind, &other.at, other.before, other.now, &other.page)
                == (&rule.kind, &rule.at, rule.before, rule.now, &rule.page)
        }) {
            return Err(format!("line {line}: this rule is listed twice"));
        }
        rules.push(rule);
    }
    let against = against.ok_or("there is no `against<TAB>version` line")?;
    Ok((against, rules))
}

/// Everything that is wrong, given the differences found against the release `released`:
/// a difference no rule takes, and a rule that takes no difference.
fn judge(differences: &[Difference], against: &str, rules: &[Rule], released: &str) -> Vec<String> {
    // Rules written against another release say nothing about this one.
    let rules: &[Rule] = if against == released { rules } else { &[] };
    let mut wrong = Vec::new();
    for difference in differences {
        if !rules.iter().any(|rule| rule.takes(difference)) {
            wrong.push(format!(
                "not listed: {} {} at {}: {} on {released}, {} here",
                difference.kind,
                difference.place,
                difference.at,
                difference.before.word(),
                difference.now.word()
            ));
        }
    }
    for rule in rules {
        if !differences.iter().any(|difference| rule.takes(difference)) {
            wrong.push(format!(
                "listed on line {} and not found: {} at {} ({} on {released}, {} here)",
                rule.line,
                rule.kind,
                rule.at,
                rule.before.word(),
                rule.now.word()
            ));
        }
    }
    if !wrong.is_empty() && against != released {
        wrong.push(format!(
            "the rules in expected_changes.tsv are written against {against} and the release here is \
             {released}, so none of them applies: list the differences above against {released}"
        ));
    }
    wrong
}

/// The binary of the last release, when the run has one. `ASSAY_READER_COMPARISON=required` makes
/// its absence an error; without it the comparison is skipped and says so.
fn released_binary(
    bin: Option<std::ffi::OsString>,
    required: Option<std::ffi::OsString>,
) -> Result<Option<PathBuf>, String> {
    let required = match required {
        None => false,
        Some(value) if value == "required" => true,
        Some(other) => {
            return Err(format!(
                "ASSAY_READER_COMPARISON is `{}`; the only value it takes is `required`",
                other.to_string_lossy()
            ))
        }
    };
    match bin.map(PathBuf::from) {
        Some(path) if path.is_file() => Ok(Some(path)),
        Some(path) => Err(format!(
            "ASSAY_RELEASED_BIN names {}, which is not a file",
            path.display()
        )),
        None if required => {
            Err("ASSAY_READER_COMPARISON=required and ASSAY_RELEASED_BIN is not set".to_string())
        }
        None => Ok(None),
    }
}

/// `6.27.0` from `assay 6.27.0`.
fn version_of(bin: &Path) -> String {
    let out = assay(bin)
        .arg("--version")
        .output()
        .unwrap_or_else(|e| panic!("run {} --version: {e}", bin.display()));
    let text = String::from_utf8_lossy(&out.stdout);
    text.trim()
        .strip_prefix("assay ")
        .unwrap_or_else(|| panic!("{} --version printed {text:?}", bin.display()))
        .to_string()
}

#[test]
fn the_last_release_and_this_tree_read_the_corpus_alike_except_where_listed() {
    let released = match released_binary(
        std::env::var_os("ASSAY_RELEASED_BIN"),
        std::env::var_os("ASSAY_READER_COMPARISON"),
    ) {
        Ok(Some(path)) => path,
        Ok(None) => {
            eprintln!(
                "skipped: ASSAY_RELEASED_BIN is not set, so the last release was not compared with this tree"
            );
            return;
        }
        Err(why) => panic!("{why}"),
    };
    let same_file =
        std::fs::canonicalize(&released).ok() == std::fs::canonicalize(this_tree()).ok();
    assert!(
        !same_file,
        "ASSAY_RELEASED_BIN is the binary of this tree; a comparison with itself finds nothing"
    );
    let release = version_of(&released);
    if std::env::var_os("ASSAY_READER_COMPARISON").is_some() {
        // Where the comparison is required, the release is the pinned one and no other.
        let pin = read(&repo().join(".github/assay-release-tag"));
        assert_eq!(
            format!("v{release}"),
            pin.trim(),
            "ASSAY_RELEASED_BIN reports {release} and .github/assay-release-tag pins {}",
            pin.trim()
        );
    }
    let (against, rules) = expected_changes(include_str!(
        "fixtures/reader_comparison/expected_changes.tsv"
    ))
    .unwrap_or_else(|why| panic!("expected_changes.tsv: {why}"));

    let items = comparison_items(&repo());
    assert!(
        items.len() > 500,
        "the corpus has {} texts; it had 889 when this test was written",
        items.len()
    );
    let read = verdicts([&released, this_tree()], &items);
    let differences: Vec<Difference> = items
        .iter()
        .zip(&read)
        .filter(|(_, [before, now])| before != now)
        .map(|(item, [before, now])| Difference {
            kind: format!("{:?}", item.kind),
            place: item.place.clone(),
            at: match item.planted.as_deref() {
                None => AS_WRITTEN.to_string(),
                Some("") => ROOT.to_string(),
                Some(path) => path.to_string(),
            },
            before: *before,
            now: *now,
        })
        .collect();
    if let Some(path) = std::env::var_os("ASSAY_READER_COMPARISON_REPORT") {
        let report: String = differences
            .iter()
            .map(|d| {
                format!(
                    "{}\t{}\t{}\t{}\t{}\n",
                    d.kind,
                    d.place,
                    d.at,
                    d.before.word(),
                    d.now.word()
                )
            })
            .collect();
        std::fs::write(path, report).expect("write the report");
    }
    eprintln!(
        "compared {} texts with release {release}: {} read differently, {} rule(s) against {against}",
        items.len(),
        differences.len(),
        rules.len()
    );
    let wrong = judge(&differences, &against, &rules, &release);
    assert!(
        wrong.is_empty(),
        "this tree and release {release} do not read the corpus alike where \
         tests/fixtures/reader_comparison/expected_changes.tsv says they do. A reader that changes a \
         verdict lists the change there, and tells a user what to do under Migration in CHANGELOG.md.\n\n{}",
        wrong.join("\n")
    );
}

#[test]
fn the_list_of_expected_changes_is_read_strictly() {
    let (against, rules) = expected_changes(
        "# a comment\nagainst\t6.27.0\nEval\ttests.*.expected.rules.*\tloaded\trefused\n\
         Mcp\tas-written\trefused\tloaded\tdocs/reference/policies.md\n",
    )
    .expect("a well-formed list");
    assert_eq!(against, "6.27.0");
    assert_eq!(rules.len(), 2);
    assert_eq!(rules[1].page.as_deref(), Some("docs/reference/policies.md"));
    for (text, what) in [
        ("Eval\t(root)\tloaded\trefused\n", "no against line"),
        ("against\t1\nagainst\t2\n", "two against lines"),
        ("against\t1\nEval\t(root)\tloaded\n", "three fields"),
        (
            "against\t1\nEval (root) loaded refused\n",
            "spaces for tabs",
        ),
        (
            "against\t1\nPolicy\t(root)\tloaded\trefused\n",
            "not a kind",
        ),
        (
            "against\t1\nEval\t(root)\tloaded\tLoaded\n",
            "not a verdict",
        ),
        ("against\t1\nEval\t(root)\tloaded\tloaded\n", "not a change"),
        (
            "against\t1\nEval\tas-written\tloaded\trefused\n",
            "as-written without a page",
        ),
        (
            "against\t1\nEval\t(root)\tloaded\trefused\tdocs/x.md\n",
            "a page without as-written",
        ),
        (
            "against\t1\nEval\t(root)\tloaded\trefused\nEval\t(root)\tloaded\trefused\n",
            "a rule twice",
        ),
    ] {
        assert!(expected_changes(text).is_err(), "accepted: {what}");
    }
}

#[test]
fn a_difference_needs_a_rule_and_a_rule_needs_a_difference() {
    let difference = |kind: &str, place: &str, at: &str| Difference {
        kind: kind.to_string(),
        place: place.to_string(),
        at: at.to_string(),
        before: Verdict::Loaded,
        now: Verdict::Refused,
    };
    let (against, rules) = expected_changes(
        "against\t6.27.0\nEval\ttests.*.expected.rules.*\tloaded\trefused\n\
         Mcp\tas-written\tloaded\trefused\tdocs/reference/policies.md\n",
    )
    .expect("rules");
    let in_rules = difference("Eval", "docs/a.md:12", "tests.*.expected.rules.*");
    let as_written = difference("Mcp", "docs/reference/policies.md:40", AS_WRITTEN);
    let both = [in_rules.clone(), as_written.clone()];

    assert_eq!(
        judge(&both, &against, &rules, "6.27.0"),
        Vec::<String>::new()
    );

    // One rule covers every document that changes in that place.
    let again = [
        in_rules.clone(),
        difference("Eval", "docs/b.md:7", "tests.*.expected.rules.*"),
        as_written.clone(),
    ];
    assert!(judge(&again, &against, &rules, "6.27.0").is_empty());

    // A difference in another place, in another kind, in the other direction, or on another page
    // is not listed.
    let mut other_way = in_rules.clone();
    (other_way.before, other_way.now) = (Verdict::Refused, Verdict::Loaded);
    for unlisted in [
        difference("Eval", "docs/a.md:12", "tests.*.expected"),
        difference("Sequence", "docs/a.md:12", "tests.*.expected.rules.*"),
        other_way,
        difference("Mcp", "docs/other.md:40", AS_WRITTEN),
    ] {
        let found = [in_rules.clone(), as_written.clone(), unlisted];
        let wrong = judge(&found, &against, &rules, "6.27.0");
        assert_eq!(wrong.len(), 1, "{wrong:?}");
        assert!(wrong[0].starts_with("not listed:"), "{wrong:?}");
    }

    // A rule with nothing behind it is as wrong as a difference with no rule.
    let wrong = judge(std::slice::from_ref(&in_rules), &against, &rules, "6.27.0");
    assert_eq!(wrong.len(), 1, "{wrong:?}");
    assert!(
        wrong[0].starts_with("listed on line 3 and not found:"),
        "{wrong:?}"
    );

    // Against another release the rules do not apply: nothing found is fine, and anything found
    // is unlisted whatever the rules say.
    assert!(judge(&[], &against, &rules, "6.28.0").is_empty());
    let wrong = judge(&both, &against, &rules, "6.28.0");
    assert_eq!(wrong.len(), 3, "{wrong:?}");
    assert!(wrong[2].contains("written against 6.27.0"), "{wrong:?}");
}

#[test]
fn the_comparison_cannot_be_skipped_where_it_is_required() {
    let this = Some(this_tree().as_os_str().to_os_string());
    let required = Some(std::ffi::OsString::from("required"));
    assert_eq!(released_binary(None, None), Ok(None));
    assert_eq!(
        released_binary(this.clone(), None),
        Ok(Some(this_tree().to_path_buf()))
    );
    assert_eq!(
        released_binary(this.clone(), required.clone()),
        Ok(Some(this_tree().to_path_buf()))
    );
    assert!(released_binary(None, required.clone()).is_err());
    assert!(released_binary(Some("/nonexistent/assay".into()), None).is_err());
    assert!(released_binary(Some("/nonexistent/assay".into()), required).is_err());
    // A value that is not `required` is a typo, not a way to turn the comparison off.
    assert!(released_binary(this, Some("true".into())).is_err());
}
