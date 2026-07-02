//! EXPERIMENTAL: import a skill supply-chain carrier into an evidence bundle.
//!
//! A carrier (`assay.skill_supply_chain.v0`) is a retained skill record: root identity, per-source
//! coverage flags, a closed reason vocabulary, and source-classed security signals. The contract is
//! pinned consumer-first (Assay-Harness `PLAN-SKILL-SUPPLY-CHAIN-SUFFICIENCY-2026Q3.md`; the Plimsoll
//! consumer pin fixes the same vocabularies byte-identically). This importer validates a single
//! supplied carrier fail-closed and writes it as one event into a bundle. Semantic verification is the
//! separate `evidence verify-skill-supply-chain` command.
//!
//! The import gate re-derives the verdict from the reason codes under the pinned worst-wins
//! precedence and rejects any carrier whose declared verdict disagrees: reason codes are decision
//! inputs, not disclosure. Dependency-detail fields (`declared_dependencies`, `reachable_dependencies`)
//! are carried opaquely in v0; the capture producer pins their shape when it lands.

use crate::exit_codes;
use anyhow::{bail, Context, Result};
use assay_evidence::bundle::BundleWriter;
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use clap::Args;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

/// Schema id of the carrier this importer accepts (also the event type).
pub(crate) const CARRIER_SCHEMA: &str = "assay.skill_supply_chain.v0";
pub(crate) const CARRIER_EVENT_TYPE: &str = "assay.skill_supply_chain.v0";
const EVENT_SOURCE: &str = "urn:assay:external:skill-supply-chain";
const DEFAULT_RUN_ID: &str = "import-skill-supply-chain";

/// The five verdicts, in worst-wins precedence order.
const VERDICTS: &[&str] = &[
    "invalid",
    "transitive_risk_present",
    "review_incomplete",
    "review_ambiguous",
    "review_complete",
];
/// Closed reason vocabulary (byte-identical to the pinned consumer contract).
const INTEGRITY_REASONS: &[&str] = &["digest_mismatch", "unsafe_skill_path"];
const RISK_REASONS: &[&str] = &["hidden_package_inventory", "known_risk_signal_reachable"];
const COVERAGE_REASONS: &[&str] = &[
    "missing_package_version",
    "missing_lockfile_evidence",
    "missing_service_endpoint",
    "unversioned_cluster_member",
    "traversal_not_retained",
];
const AMBIGUITY_REASONS: &[&str] = &["unresolved_text_dependency"];
/// The five per-source coverage flags a producer must answer honestly.
const COVERAGE_KEYS: &[&str] = &[
    "front_matter",
    "body_text",
    "scripts",
    "lockfiles",
    "transitive_traversal",
];
const COVERAGE_VALUES: &[&str] = &["present", "not_present", "not_applicable"];
const SIGNAL_KINDS: &[&str] = &["occurrence", "absence"];

#[derive(Debug, Args, Clone)]
pub struct SkillSupplyChainArgs {
    /// Skill supply-chain carrier JSON artifact (`assay.skill_supply_chain.v0`)
    #[arg(long, value_name = "PATH")]
    pub carrier: PathBuf,

    /// Output Assay evidence bundle path (.tar.gz)
    #[arg(long, alias = "out", value_name = "PATH")]
    pub bundle_out: PathBuf,

    /// Assay import run id used for provenance and event ids
    #[arg(long, default_value = DEFAULT_RUN_ID)]
    pub run_id: String,

    /// Import timestamp for deterministic fixtures (RFC3339 UTC recommended)
    #[arg(long)]
    pub import_time: Option<String>,
}

#[derive(Debug, Args, Clone)]
pub struct CaptureSkillSupplyChainArgs {
    /// Local skill file or skill directory to retain into a carrier
    #[arg(long, value_name = "PATH")]
    pub skill_root: PathBuf,

    /// Canonical in-repository root path to record in the carrier, e.g. skills/release-notes
    #[arg(long, value_name = "REL_PATH")]
    pub root_path: String,

    /// Output skill supply-chain carrier JSON path
    #[arg(long, alias = "out", value_name = "PATH")]
    pub carrier_out: PathBuf,
}

pub fn cmd_skill_supply_chain(args: SkillSupplyChainArgs) -> Result<i32> {
    if args.run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }
    let import_time = parse_import_time(args.import_time.as_deref())?;
    let producer = ProducerMeta {
        name: "assay-cli".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        git: option_env!("ASSAY_GIT_SHA").map(str::to_string),
    };

    let carrier_bytes = fs::read_to_string(&args.carrier)
        .with_context(|| format!("failed to read carrier {}", args.carrier.display()))?;
    let carrier: Value = serde_json::from_str(&carrier_bytes)
        .with_context(|| format!("failed to parse carrier {}", args.carrier.display()))?;

    validate_carrier(&carrier)?;

    let carrier_event =
        EvidenceEvent::new(CARRIER_EVENT_TYPE, EVENT_SOURCE, &args.run_id, 0, carrier)
            .with_time(import_time)
            .with_producer(&producer);

    let out_file = File::create(&args.bundle_out)
        .with_context(|| format!("failed to create bundle {}", args.bundle_out.display()))?;
    let mut writer = BundleWriter::new(out_file).with_producer(producer);
    writer.add_event(carrier_event);
    writer
        .finish()
        .with_context(|| format!("failed to write bundle {}", args.bundle_out.display()))?;

    eprintln!(
        "Imported skill supply-chain carrier to {}",
        args.bundle_out.display()
    );
    Ok(exit_codes::OK)
}

pub fn cmd_capture_skill_supply_chain(args: CaptureSkillSupplyChainArgs) -> Result<i32> {
    let carrier = capture_carrier_from_skill_root(&args.skill_root, &args.root_path)?;
    let mut rendered = serde_json::to_string_pretty(&carrier)?;
    rendered.push('\n');
    fs::write(&args.carrier_out, rendered)
        .with_context(|| format!("failed to write carrier {}", args.carrier_out.display()))?;
    eprintln!(
        "Captured skill supply-chain carrier to {}",
        args.carrier_out.display()
    );
    Ok(exit_codes::OK)
}

#[derive(Debug)]
struct RetainedSkillFile {
    relative_path: String,
    bytes: Vec<u8>,
}

pub(crate) fn capture_carrier_from_skill_root(skill_root: &Path, root_path: &str) -> Result<Value> {
    if !is_safe_skill_path(root_path) {
        bail!("capture root_path {root_path:?} is unsafe");
    }

    let metadata = fs::symlink_metadata(skill_root)
        .with_context(|| format!("failed to inspect skill root {}", skill_root.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "skill root {} is a symlink; refusing to capture",
            skill_root.display()
        );
    }

    let mut retained_files = Vec::new();
    if metadata.is_file() {
        let file_name = skill_root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::anyhow!("skill root must have a UTF-8 file name"))?;
        retained_files.push(RetainedSkillFile {
            relative_path: file_name.to_string(),
            bytes: fs::read(skill_root)
                .with_context(|| format!("failed to read skill file {}", skill_root.display()))?,
        });
    } else if metadata.is_dir() {
        collect_retained_skill_files(skill_root, skill_root, &mut retained_files)?;
    } else {
        bail!(
            "skill root {} must be a regular file or directory",
            skill_root.display()
        );
    }

    if retained_files.is_empty() {
        bail!(
            "skill root {} did not contain retained files",
            skill_root.display()
        );
    }
    retained_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));

    let root_name = root_path
        .rsplit('/')
        .find(|part| !part.is_empty())
        .ok_or_else(|| anyhow::anyhow!("root_path {root_path:?} must name a skill root"))?;
    let retained_paths = retained_files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();
    let front_matter = coverage_flag(retained_files.iter().any(has_front_matter));
    let body_text = coverage_flag(retained_files.iter().any(has_body_text));
    let scripts = if retained_files
        .iter()
        .any(|file| is_script_file(&file.relative_path))
    {
        "present"
    } else {
        "not_applicable"
    };
    let lockfiles = if retained_files
        .iter()
        .any(|file| is_lockfile(&file.relative_path))
    {
        "present"
    } else {
        "not_applicable"
    };

    let carrier = json!({
        "schema": CARRIER_SCHEMA,
        "root": {
            "name": root_name,
            "path": root_path,
            "digest": digest_retained_files(&retained_files),
            "retained_files": retained_paths,
        },
        "verdict": "review_incomplete",
        "reason_codes": ["traversal_not_retained"],
        "coverage": {
            "front_matter": front_matter,
            "body_text": body_text,
            "scripts": scripts,
            "lockfiles": lockfiles,
            "transitive_traversal": "not_present",
        },
        "signals": [],
        "non_claims": [
            "review_complete_is_not_skill_safe",
            "verdict_covers_one_root_at_one_capture_time",
            "capture_does_not_scan_registries",
            "capture_does_not_resolve_transitive_dependencies",
            "capture_does_not_detect_malware",
        ],
    });
    validate_carrier(&carrier)?;
    Ok(carrier)
}

fn collect_retained_skill_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<RetainedSkillFile>,
) -> Result<()> {
    let mut entries = fs::read_dir(current)
        .with_context(|| format!("failed to read skill directory {}", current.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("failed to list skill directory {}", current.display()))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to inspect skill path {}", path.display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "skill path {} is a symlink; refusing to capture",
                path.display()
            );
        }
        if metadata.is_dir() {
            collect_retained_skill_files(root, &path, files)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(root)
                .expect("walked paths stay under root")
                .to_string_lossy()
                .replace('\\', "/");
            files.push(RetainedSkillFile {
                relative_path: relative,
                bytes: fs::read(&path)
                    .with_context(|| format!("failed to read skill file {}", path.display()))?,
            });
        }
    }
    Ok(())
}

fn digest_retained_files(files: &[RetainedSkillFile]) -> String {
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(file.relative_path.as_bytes());
        hasher.update(b"\0");
        hasher.update((file.bytes.len() as u64).to_be_bytes());
        hasher.update(b"\0");
        hasher.update(&file.bytes);
        hasher.update(b"\0");
    }
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn coverage_flag(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "not_present"
    }
}

fn has_front_matter(file: &RetainedSkillFile) -> bool {
    if !is_markdown_file(&file.relative_path) {
        return false;
    }
    let text = String::from_utf8_lossy(&file.bytes);
    let Some(rest) = text.strip_prefix("---\n") else {
        return false;
    };
    rest.lines().any(|line| line.trim() == "---")
}

fn has_body_text(file: &RetainedSkillFile) -> bool {
    if !is_markdown_file(&file.relative_path) {
        return false;
    }
    let text = String::from_utf8_lossy(&file.bytes);
    !strip_front_matter(&text).trim().is_empty()
}

fn strip_front_matter(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    let mut offset = 4;
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim() == "---" {
            return &text[offset..];
        }
    }
    text
}

fn is_markdown_file(path: &str) -> bool {
    path.eq_ignore_ascii_case("SKILL.md") || path.to_ascii_lowercase().ends_with(".md")
}

fn is_script_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("scripts/")
        || matches!(
            lower.rsplit('.').next(),
            Some("sh" | "bash" | "zsh" | "py" | "js" | "mjs" | "cjs" | "ts" | "tsx")
        )
}

fn is_lockfile(path: &str) -> bool {
    matches!(
        path.rsplit('/').next(),
        Some(
            "Cargo.lock"
                | "Gemfile.lock"
                | "Pipfile.lock"
                | "bun.lockb"
                | "package-lock.json"
                | "pnpm-lock.yaml"
                | "poetry.lock"
                | "requirements.lock"
                | "uv.lock"
                | "yarn.lock"
        )
    )
}

/// The verdict the reason codes require under the pinned worst-wins precedence.
pub(crate) fn expected_verdict(reasons: &[&str]) -> &'static str {
    if reasons.iter().any(|r| INTEGRITY_REASONS.contains(r)) {
        "invalid"
    } else if reasons.iter().any(|r| RISK_REASONS.contains(r)) {
        "transitive_risk_present"
    } else if reasons.iter().any(|r| COVERAGE_REASONS.contains(r)) {
        "review_incomplete"
    } else if reasons.iter().any(|r| AMBIGUITY_REASONS.contains(r)) {
        "review_ambiguous"
    } else {
        "review_complete"
    }
}

/// `true` when a root path is safe under the writer conventions: relative, POSIX separators, no
/// `..` components, no backslashes, not empty.
fn is_safe_skill_path(path: &str) -> bool {
    let has_drive_prefix = path.len() >= 2 && path.as_bytes()[1] == b':';
    let unsafe_path = path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || has_drive_prefix
        || path.split('/').any(|c| c == "..");
    !unsafe_path
}

/// Fail-closed carrier validation against the pinned contract. An incoherent carrier never reaches
/// the bundle: the declared verdict must re-derive from the reason codes, occurrence and absence
/// signals must corroborate the reasons, and coverage flags must be answered from the closed enum.
pub(crate) fn validate_carrier(carrier: &Value) -> Result<()> {
    let obj = carrier
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("carrier must be a JSON object"))?;

    match obj.get("schema").and_then(Value::as_str) {
        Some(CARRIER_SCHEMA) => {}
        Some(other) => bail!("carrier schema must be {CARRIER_SCHEMA:?}, got {other:?}"),
        None => bail!("carrier missing string schema"),
    }

    // Root identity: name + path-safety. An unsafe path may be retained as evidence, but only when
    // the record itself says so (unsafe_skill_path -> verdict invalid via worst-wins).
    let root = obj
        .get("root")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("carrier missing root object"))?;
    let root_name = root
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("carrier root.name must be a non-empty string"))?;
    let _ = root_name;
    let root_path = root
        .get("path")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("carrier root.path must be a non-empty string"))?;

    let verdict = obj
        .get("verdict")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("carrier missing string verdict"))?;
    if !VERDICTS.contains(&verdict) {
        bail!(
            "carrier verdict {verdict:?} is not one of {}",
            VERDICTS.join("|")
        );
    }

    let reasons_value = obj
        .get("reason_codes")
        .ok_or_else(|| anyhow::anyhow!("carrier missing reason_codes array"))?;
    let reasons: Vec<&str> = match reasons_value.as_array() {
        Some(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(item.as_str().ok_or_else(|| {
                    anyhow::anyhow!("carrier reason_codes must be an array of strings")
                })?);
            }
            out
        }
        None => bail!("carrier reason_codes must be an array of strings"),
    };
    for reason in &reasons {
        let known = INTEGRITY_REASONS.contains(reason)
            || RISK_REASONS.contains(reason)
            || COVERAGE_REASONS.contains(reason)
            || AMBIGUITY_REASONS.contains(reason);
        if !known {
            bail!("carrier reason code {reason:?} is not in the closed vocabulary; unknown reason codes reject the record");
        }
    }

    // The verdict is a derived value, never free-standing disclosure.
    let expected = expected_verdict(&reasons);
    if verdict != expected {
        bail!(
            "carrier verdict {verdict:?} does not match reason codes under worst-wins precedence (expected {expected:?})"
        );
    }

    // Path-safety coherence: an unsafe root path must be declared as such.
    if !is_safe_skill_path(root_path) && !reasons.contains(&"unsafe_skill_path") {
        bail!("carrier root.path {root_path:?} is unsafe but unsafe_skill_path is not declared");
    }

    // Coverage: exactly the five flags, each answered from the closed enum. Silence is not permitted.
    let coverage = obj
        .get("coverage")
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow::anyhow!("carrier missing coverage object"))?;
    for key in COVERAGE_KEYS {
        match coverage.get(*key).and_then(Value::as_str) {
            Some(value) if COVERAGE_VALUES.contains(&value) => {}
            Some(value) => bail!(
                "carrier coverage.{key} {value:?} is not one of {}",
                COVERAGE_VALUES.join("|")
            ),
            None => bail!(
                "carrier coverage.{key} must be one of {}",
                COVERAGE_VALUES.join("|")
            ),
        }
    }
    for key in coverage.keys() {
        if !COVERAGE_KEYS.contains(&key.as_str()) {
            bail!("carrier coverage has unknown key {key:?}");
        }
    }
    let traversal = coverage
        .get("transitive_traversal")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if verdict == "review_complete" && traversal != "present" {
        bail!(
            "carrier verdict review_complete requires coverage.transitive_traversal to be present"
        );
    }

    // Signals: occurrence needs a risk reason; a known-signal reason needs an occurrence signal; an
    // absence statement needs a machine-readable justification and cannot appear with risk findings.
    let signals = match obj.get("signals") {
        None => Vec::new(),
        Some(Value::Array(items)) => items.iter().collect::<Vec<_>>(),
        Some(_) => bail!("carrier signals must be an array of objects"),
    };
    let mut occurrence_count = 0usize;
    let mut absence_count = 0usize;
    for signal in &signals {
        let sig = signal
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("carrier signals must be an array of objects"))?;
        let kind = sig
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("carrier signal missing string kind"))?;
        if !SIGNAL_KINDS.contains(&kind) {
            bail!("carrier signal kind {kind:?} is not one of occurrence|absence");
        }
        sig.get("source_class")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("carrier signal source_class must be a non-empty string")
            })?;
        if kind == "occurrence" {
            occurrence_count += 1;
        } else {
            absence_count += 1;
            sig.get("justification")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "carrier absence signal must carry a non-empty machine-readable justification"
                    )
                })?;
        }
    }
    let has_risk_reason = reasons.iter().any(|r| RISK_REASONS.contains(r));
    if occurrence_count > 0 && !has_risk_reason {
        bail!("carrier occurrence signal requires a risk reason code");
    }
    if reasons.contains(&"known_risk_signal_reachable") && occurrence_count == 0 {
        bail!("carrier known_risk_signal_reachable requires an occurrence signal");
    }
    if absence_count > 0 && has_risk_reason {
        bail!("carrier absence signal cannot appear with risk findings");
    }

    // The claim ceiling travels with the record.
    match obj.get("non_claims").and_then(Value::as_array) {
        Some(items) if !items.is_empty() && items.iter().all(|v| v.is_string()) => {}
        _ => bail!("carrier non_claims must be a non-empty array of strings"),
    }

    Ok(())
}

fn parse_import_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(value) => Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("invalid --import-time {value:?}; expected RFC3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use assay_evidence::bundle::BundleReader;
    use serde_json::json;

    pub(crate) fn sample_carrier() -> Value {
        json!({
            "schema": CARRIER_SCHEMA,
            "root": {"name": "release-notes", "path": "skills/release-notes"},
            "verdict": "review_complete",
            "reason_codes": [],
            "coverage": {
                "front_matter": "present",
                "body_text": "present",
                "scripts": "present",
                "lockfiles": "present",
                "transitive_traversal": "present"
            },
            "signals": [],
            "non_claims": [
                "review_complete_is_not_skill_safe",
                "verdict_covers_one_root_at_one_capture_time"
            ]
        })
    }

    pub(crate) fn run_import(
        carrier: &Value,
        run_id: &str,
    ) -> Result<(std::path::PathBuf, tempfile::TempDir)> {
        let dir = tempfile::tempdir().unwrap();
        let carrier_path = dir.path().join("carrier.json");
        let out = dir.path().join("ssc.tar.gz");
        fs::write(
            &carrier_path,
            serde_json::to_string_pretty(carrier).unwrap(),
        )
        .unwrap();
        cmd_skill_supply_chain(SkillSupplyChainArgs {
            carrier: carrier_path,
            bundle_out: out.clone(),
            run_id: run_id.to_string(),
            import_time: Some("2026-07-02T00:00:00Z".to_string()),
        })
        .map(|_| (out, dir))
    }

    #[test]
    fn capture_local_skill_tree_emits_honest_incomplete_carrier() {
        let dir = tempfile::tempdir().unwrap();
        let skill_root = dir.path().join("skills/eval-esser");
        fs::create_dir_all(&skill_root).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: eval-esser\n---\n\n# Eval Esser\n\nReview eval evidence.\n",
        )
        .unwrap();

        let carrier = capture_carrier_from_skill_root(&skill_root, "skills/eval-esser").unwrap();

        assert_eq!(carrier["schema"], json!(CARRIER_SCHEMA));
        assert_eq!(carrier["root"]["name"], json!("eval-esser"));
        assert_eq!(carrier["root"]["path"], json!("skills/eval-esser"));
        assert!(carrier["root"]["digest"]
            .as_str()
            .unwrap()
            .starts_with("sha256:"));
        assert_eq!(carrier["root"]["retained_files"], json!(["SKILL.md"]));
        assert_eq!(carrier["verdict"], json!("review_incomplete"));
        assert_eq!(carrier["reason_codes"], json!(["traversal_not_retained"]));
        assert_eq!(carrier["coverage"]["front_matter"], json!("present"));
        assert_eq!(carrier["coverage"]["body_text"], json!("present"));
        assert_eq!(carrier["coverage"]["scripts"], json!("not_applicable"));
        assert_eq!(carrier["coverage"]["lockfiles"], json!("not_applicable"));
        assert_eq!(
            carrier["coverage"]["transitive_traversal"],
            json!("not_present")
        );
        assert_eq!(carrier["signals"], json!([]));
        assert!(carrier["non_claims"]
            .as_array()
            .unwrap()
            .iter()
            .any(|claim| claim == "capture_does_not_resolve_transitive_dependencies"));
        validate_carrier(&carrier).unwrap();
    }

    #[test]
    fn capture_command_writes_carrier_json_file() {
        let dir = tempfile::tempdir().unwrap();
        let skill_root = dir.path().join("skills/release-notes");
        fs::create_dir_all(&skill_root).unwrap();
        fs::write(
            skill_root.join("SKILL.md"),
            "---\nname: release-notes\n---\n\n# Release Notes\n\nSummarize changes.\n",
        )
        .unwrap();
        let carrier_out = dir.path().join("carrier.json");

        cmd_capture_skill_supply_chain(CaptureSkillSupplyChainArgs {
            skill_root,
            root_path: "skills/release-notes".to_string(),
            carrier_out: carrier_out.clone(),
        })
        .unwrap();

        let carrier: Value =
            serde_json::from_str(&fs::read_to_string(carrier_out).unwrap()).unwrap();
        assert_eq!(carrier["root"]["path"], json!("skills/release-notes"));
        assert_eq!(carrier["verdict"], json!("review_incomplete"));
        validate_carrier(&carrier).unwrap();
    }

    #[test]
    fn import_writes_single_carrier_event_bundle() {
        let (out, _dir) = run_import(&sample_carrier(), "ssc_test").unwrap();
        let reader = BundleReader::open(File::open(&out).unwrap()).unwrap();
        assert_eq!(reader.manifest().event_count, 1);
        let events = reader.events_vec().unwrap();
        assert_eq!(events[0].type_, CARRIER_EVENT_TYPE);
        assert_eq!(events[0].payload["verdict"], json!("review_complete"));
    }

    #[test]
    fn import_accepts_all_vector_families() {
        // The lab vector families, expressed as coherent carriers.
        let mut incomplete_lockfile = sample_carrier();
        incomplete_lockfile["verdict"] = json!("review_incomplete");
        incomplete_lockfile["reason_codes"] =
            json!(["missing_package_version", "missing_lockfile_evidence"]);
        incomplete_lockfile["coverage"]["lockfiles"] = json!("not_present");

        let mut incomplete_endpoint = sample_carrier();
        incomplete_endpoint["verdict"] = json!("review_incomplete");
        incomplete_endpoint["reason_codes"] = json!(["missing_service_endpoint"]);

        let mut incomplete_cluster = sample_carrier();
        incomplete_cluster["verdict"] = json!("review_incomplete");
        incomplete_cluster["reason_codes"] = json!(["unversioned_cluster_member"]);

        let mut ambiguous = sample_carrier();
        ambiguous["verdict"] = json!("review_ambiguous");
        ambiguous["reason_codes"] = json!(["unresolved_text_dependency"]);

        let mut hidden_inventory = sample_carrier();
        hidden_inventory["verdict"] = json!("transitive_risk_present");
        hidden_inventory["reason_codes"] = json!(["hidden_package_inventory"]);

        let mut known_signal = sample_carrier();
        known_signal["verdict"] = json!("transitive_risk_present");
        known_signal["reason_codes"] = json!(["known_risk_signal_reachable"]);
        known_signal["signals"] = json!([
            {"kind": "occurrence", "source_class": "registry_scanner_report"}
        ]);

        let mut invalid_digest = sample_carrier();
        invalid_digest["verdict"] = json!("invalid");
        invalid_digest["reason_codes"] = json!(["digest_mismatch"]);

        // Risk reports at any coverage level: risk + missing coverage stays risk.
        let mut risk_partial = sample_carrier();
        risk_partial["verdict"] = json!("transitive_risk_present");
        risk_partial["reason_codes"] =
            json!(["hidden_package_inventory", "missing_lockfile_evidence"]);
        risk_partial["coverage"]["lockfiles"] = json!("not_present");
        risk_partial["coverage"]["transitive_traversal"] = json!("not_present");

        for (name, carrier) in [
            ("incomplete_lockfile", incomplete_lockfile),
            ("incomplete_endpoint", incomplete_endpoint),
            ("incomplete_cluster", incomplete_cluster),
            ("ambiguous", ambiguous),
            ("hidden_inventory", hidden_inventory),
            ("known_signal", known_signal),
            ("invalid_digest", invalid_digest),
            ("risk_partial", risk_partial),
        ] {
            run_import(&carrier, "ssc_test")
                .unwrap_or_else(|e| panic!("family {name} must import: {e}"));
        }
    }

    #[test]
    fn import_rejects_unknown_reason_code() {
        let mut carrier = sample_carrier();
        carrier["verdict"] = json!("review_incomplete");
        carrier["reason_codes"] = json!(["novel_reason"]);
        let err = run_import(&carrier, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("closed vocabulary"));
    }

    #[test]
    fn import_rejects_verdict_reason_mismatch_both_directions() {
        // Coverage reason relabelled as risk.
        let mut relabel = sample_carrier();
        relabel["verdict"] = json!("transitive_risk_present");
        relabel["reason_codes"] = json!(["missing_lockfile_evidence"]);
        let err = run_import(&relabel, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("worst-wins"));

        // Risk downgraded for coverage.
        let mut downgrade = sample_carrier();
        downgrade["verdict"] = json!("review_incomplete");
        downgrade["reason_codes"] =
            json!(["hidden_package_inventory", "missing_lockfile_evidence"]);
        let err = run_import(&downgrade, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("worst-wins"));

        // Integrity dominates risk.
        let mut integrity = sample_carrier();
        integrity["verdict"] = json!("transitive_risk_present");
        integrity["reason_codes"] = json!(["digest_mismatch", "hidden_package_inventory"]);
        let err = run_import(&integrity, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("expected \"invalid\""));
    }

    #[test]
    fn import_rejects_signal_incoherence() {
        // Occurrence signal without a risk reason.
        let mut occurrence = sample_carrier();
        occurrence["signals"] =
            json!([{"kind": "occurrence", "source_class": "registry_scanner_report"}]);
        let err = run_import(&occurrence, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("requires a risk reason code"));

        // known_risk_signal_reachable without an occurrence signal.
        let mut no_signal = sample_carrier();
        no_signal["verdict"] = json!("transitive_risk_present");
        no_signal["reason_codes"] = json!(["known_risk_signal_reachable"]);
        let err = run_import(&no_signal, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("requires an occurrence signal"));

        // Absence signal alongside risk findings.
        let mut contradiction = sample_carrier();
        contradiction["verdict"] = json!("transitive_risk_present");
        contradiction["reason_codes"] = json!(["hidden_package_inventory"]);
        contradiction["signals"] = json!([
            {"kind": "absence", "source_class": "boundary_observed",
             "justification": "reviewed_boundary_fully_traversed"}
        ]);
        let err = run_import(&contradiction, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("cannot appear with risk findings"));

        // Bare absence statement (no justification).
        let mut bare = sample_carrier();
        bare["signals"] = json!([{"kind": "absence", "source_class": "boundary_observed"}]);
        let err = run_import(&bare, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("machine-readable justification"));
    }

    #[test]
    fn import_rejects_dishonest_coverage() {
        // review_complete without retained traversal.
        let mut no_traversal = sample_carrier();
        no_traversal["coverage"]["transitive_traversal"] = json!("not_present");
        let err = run_import(&no_traversal, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("transitive_traversal"));

        // Out-of-enum coverage value.
        let mut bad_value = sample_carrier();
        bad_value["coverage"]["lockfiles"] = json!("unknown");
        let err = run_import(&bad_value, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("coverage.lockfiles"));

        // Unknown coverage key.
        let mut extra_key = sample_carrier();
        extra_key["coverage"]["network"] = json!("present");
        let err = run_import(&extra_key, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("unknown key"));

        // Missing flag (silence is not permitted).
        let mut silent = sample_carrier();
        silent["coverage"]
            .as_object_mut()
            .unwrap()
            .remove("scripts");
        let err = run_import(&silent, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("coverage.scripts"));
    }

    #[test]
    fn import_rejects_undeclared_unsafe_root_path() {
        for bad in ["/abs/skill", "skills/../escape", "skills\\win"] {
            let mut carrier = sample_carrier();
            carrier["root"]["path"] = json!(bad);
            let err = run_import(&carrier, "ssc_test").unwrap_err();
            assert!(
                err.to_string().contains("unsafe"),
                "path {bad:?} must be rejected: {err}"
            );
        }

        // The same path IS importable when the record declares it and the verdict follows.
        let mut declared = sample_carrier();
        declared["root"]["path"] = json!("/abs/skill");
        declared["verdict"] = json!("invalid");
        declared["reason_codes"] = json!(["unsafe_skill_path"]);
        run_import(&declared, "ssc_test").unwrap();
    }

    #[test]
    fn import_rejects_missing_non_claims_and_bad_run_id() {
        let mut carrier = sample_carrier();
        carrier["non_claims"] = json!([]);
        let err = run_import(&carrier, "ssc_test").unwrap_err();
        assert!(err.to_string().contains("non_claims"));

        let err = run_import(&sample_carrier(), "bad:run").unwrap_err();
        assert!(err.to_string().contains("run_id cannot contain ':'"));
    }
}
