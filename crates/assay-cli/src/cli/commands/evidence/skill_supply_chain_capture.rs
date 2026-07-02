//! EXPERIMENTAL: honest-declarative capture of a skill root into an assay.skill_supply_chain.v0 carrier.
//!
//! This is the skill-channel producer (DoR implementation shelf item 3): it walks ONE skill root,
//! answers the five coverage flags from what is actually on disk, reads STRUCTURED declared
//! dependencies from front matter (skill/package/service channels), and emits a carrier whose verdict
//! recomputes under worst-wins. It is deliberately NOT an analyzer: it does not extract dependencies
//! from natural-language prose, does not resolve or execute anything, and attaches no security signals
//! (those enter via `evidence adapt-skill-scan`). Missing evidence is reported as `not_present`, never a
//! silent pass. The emitted carrier is re-validated with the same gate the importer uses, so a captured
//! carrier is guaranteed importable.

use super::skill_supply_chain::{expected_verdict, validate_carrier};
use crate::exit_codes;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

const CARRIER_SCHEMA: &str = "assay.skill_supply_chain.v0";
const NON_CLAIMS: &[&str] = &[
    "review_complete_is_not_skill_safe",
    "transitive_risk_present_is_not_skill_malicious",
    "verdict_covers_one_root_at_one_capture_time",
    "no_registry_wide_claim",
    "no_runtime_behavior_claim",
    "capture_is_declarative_not_an_analyzer",
];
/// Adjacent files that count as retained lockfile evidence for the package channel.
const LOCKFILE_NAMES: &[&str] = &[
    "package-lock.json",
    "requirements.txt",
    "uv.lock",
    "Cargo.lock",
    "poetry.lock",
    "pnpm-lock.yaml",
    "yarn.lock",
];

#[derive(Debug, clap::Args, Clone)]
pub struct CaptureSkillSupplyChainArgs {
    /// Skill root: a SKILL.md / agent markdown file, or a directory containing one
    #[arg(long, value_name = "PATH")]
    pub root: PathBuf,

    /// Where to write the carrier JSON (stdout if omitted)
    #[arg(long, value_name = "PATH")]
    pub out: Option<PathBuf>,
}

pub fn cmd_capture_skill_supply_chain(args: CaptureSkillSupplyChainArgs) -> Result<i32> {
    let carrier = capture_carrier(&args.root)?;
    // Fail-closed: never emit a carrier the importer would reject.
    validate_carrier(&carrier).context("captured carrier failed self-validation (bug)")?;
    let json = serde_json::to_string_pretty(&carrier)?;
    match &args.out {
        Some(path) => {
            fs::write(path, format!("{json}\n"))
                .with_context(|| format!("failed to write {}", path.display()))?;
            eprintln!("Captured skill supply-chain carrier to {}", path.display());
        }
        None => println!("{json}"),
    }
    Ok(exit_codes::OK)
}

/// A skill root resolved to its markdown artifact plus the directory that holds adjacent files.
struct Resolved {
    /// The markdown file that carries front matter + body.
    markdown: PathBuf,
    /// Directory scanned for adjacent scripts and lockfiles.
    dir: PathBuf,
    /// The reviewer-facing root path (relative, POSIX) recorded in the carrier.
    root_path: String,
}

/// `true` when `path` is a symlink, checked WITHOUT following it. Capture refuses to read through a
/// symlink so a link pointing outside the reviewed tree cannot be captured as if it were the skill.
fn is_symlink(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// `true` when `path` exists and its canonical real location stays inside `base` (following any
/// symlinks and `..`). A reused-skill dependency that escapes the reviewed tree is not safely
/// traversed.
fn within_tree(path: &Path, base: &Path) -> bool {
    match (fs::canonicalize(path), fs::canonicalize(base)) {
        (Ok(real), Ok(real_base)) => real.starts_with(&real_base),
        _ => false,
    }
}

fn resolve_root(root: &Path) -> Result<Resolved> {
    let root_path = posix_relative(root);
    if is_symlink(root) {
        bail!(
            "skill root {} is a symlink; refusing to capture (a symlink can point outside the reviewed tree)",
            root.display()
        );
    }
    if root.is_file() {
        let dir = root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        return Ok(Resolved {
            markdown: root.to_path_buf(),
            dir,
            root_path,
        });
    }
    if root.is_dir() {
        // Prefer a conventional skill entry point, else the first front-matter markdown.
        for name in ["SKILL.md", "skill.md", "AGENT.md", "agent.md"] {
            let candidate = root.join(name);
            if candidate.is_file() {
                return Ok(Resolved {
                    markdown: candidate,
                    dir: root.to_path_buf(),
                    root_path,
                });
            }
        }
        let mut markdowns: Vec<PathBuf> = fs::read_dir(root)
            .with_context(|| format!("failed to read directory {}", root.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("md"))
            .collect();
        markdowns.sort();
        if let Some(md) = markdowns.into_iter().find(|p| has_front_matter(p)) {
            return Ok(Resolved {
                markdown: md,
                dir: root.to_path_buf(),
                root_path,
            });
        }
        bail!(
            "no SKILL.md or front-matter markdown found under {}",
            root.display()
        );
    }
    bail!("skill root {} does not exist", root.display())
}

fn has_front_matter(path: &Path) -> bool {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.starts_with("---\n") || s.starts_with("---\r\n"))
        .unwrap_or(false)
}

/// Split a markdown file into its YAML front-matter block and the remaining body.
fn split_front_matter(text: &str) -> (Option<&str>, &str) {
    let rest = match text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    {
        Some(rest) => rest,
        None => return (None, text),
    };
    // The closing fence is a line that is exactly `---`.
    for marker in ["\n---\n", "\r\n---\r\n", "\n---\r\n", "\r\n---\n"] {
        if let Some(idx) = rest.find(marker) {
            let front = &rest[..idx];
            let body = &rest[idx + marker.len()..];
            return (Some(front), body);
        }
    }
    // Unterminated front matter: treat the whole remainder as front matter, empty body.
    (Some(rest), "")
}

/// The reviewer-facing root path, normalized to a safe relative POSIX path (repo writer convention):
/// relative to cwd when possible, then leading slashes / drive prefixes / `..` components stripped so
/// the recorded identity always satisfies the importer's path-safety gate.
fn posix_relative(path: &Path) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let rel = path.strip_prefix(&cwd).unwrap_or(path);
    let posix = rel.to_string_lossy().replace('\\', "/");
    let cleaned: Vec<&str> = posix
        .split('/')
        .filter(|c| !c.is_empty() && *c != ".." && *c != ".")
        // Drop a Windows drive component like `C:` if it survived stripping.
        .filter(|c| !(c.len() == 2 && c.as_bytes()[1] == b':'))
        .collect();
    if cleaned.is_empty() {
        "skill".to_string()
    } else {
        cleaned.join("/")
    }
}

/// Build the carrier from a resolved skill root. All coverage answers come from the filesystem and
/// front-matter declarations; nothing is inferred from prose.
fn capture_carrier(root: &Path) -> Result<Value> {
    let resolved = resolve_root(root)?;
    // The resolved markdown may have been picked from a directory listing; refuse a symlinked entry
    // point too, so a symlinked SKILL.md inside a real directory cannot be read through.
    if is_symlink(&resolved.markdown) {
        bail!(
            "skill markdown {} is a symlink; refusing to capture",
            resolved.markdown.display()
        );
    }
    let text = fs::read_to_string(&resolved.markdown)
        .with_context(|| format!("failed to read {}", resolved.markdown.display()))?;
    let (front_raw, body) = split_front_matter(&text);

    let front: Value = match front_raw {
        Some(raw) => serde_yaml::from_str(raw).unwrap_or(Value::Null),
        None => Value::Null,
    };
    let name = front
        .get("name")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            resolved
                .markdown
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| "unknown".to_string());

    let mut reasons: Vec<String> = Vec::new();

    // --- coverage: front matter + body ---
    let front_matter = if front_raw.is_some() && front.get("name").is_some() {
        "present"
    } else {
        "not_present"
    };
    let body_text = if body.trim().is_empty() {
        "not_present"
    } else {
        "present"
    };

    // --- coverage: scripts (adjacent .sh/.py/.js/.ts or a scripts/ dir) ---
    let scripts_present = dir_has_scripts(&resolved.dir);
    let scripts = if scripts_present {
        "present"
    } else {
        "not_applicable"
    };

    // --- declared dependencies (structured front matter only, three channels) ---
    let deps = front.get("dependencies");
    let packages = channel_list(deps, "packages");
    let services = channel_list(deps, "services");
    let skills = channel_list(deps, "skills");

    // --- coverage: lockfiles (package channel) ---
    let has_lockfile = LOCKFILE_NAMES
        .iter()
        .any(|n| resolved.dir.join(n).is_file());
    let lockfiles = if packages.is_empty() {
        "not_applicable"
    } else if has_lockfile {
        "present"
    } else {
        reasons.push("missing_lockfile_evidence".to_string());
        "not_present"
    };
    // A version/endpoint is declared when the field is present and non-empty. YAML may parse a bare
    // version like `2.0` as a number, so any non-null, non-empty value counts as declared.
    if packages.iter().any(|p| !field_declared(p, "version")) {
        reasons.push("missing_package_version".to_string());
    }
    if services.iter().any(|s| !field_declared(s, "endpoint")) {
        reasons.push("missing_service_endpoint".to_string());
    }

    // --- coverage: transitive traversal (skill channel reuse) ---
    let mut reachable: Vec<Value> = Vec::new();
    let transitive_traversal = if skills.is_empty() {
        "not_applicable"
    } else {
        let mut all_resolved = true;
        for skill in &skills {
            let decl_path = skill.get("path").and_then(Value::as_str).unwrap_or("");
            // A reused-skill path is safely traversed only when it resolves to a real location that
            // stays INSIDE the reviewed tree. A symlink or `..` component that escapes the tree is the
            // hidden-inventory risk, so it counts as unresolved -> traversal_not_retained. An internal
            // symlink that stays within the tree is benign and traverses normally.
            let joined = resolved.dir.join(decl_path);
            let resolved_here = !decl_path.is_empty() && within_tree(&joined, &resolved.dir);
            if resolved_here {
                reachable.push(json!({
                    "channel": "skill",
                    "declaring_parent": name,
                    "name": skill.get("name").cloned().unwrap_or(Value::Null),
                    "path": decl_path,
                }));
            } else {
                all_resolved = false;
            }
        }
        if all_resolved {
            "present"
        } else {
            reasons.push("traversal_not_retained".to_string());
            "not_present"
        }
    };

    // Path safety is guaranteed by construction: `posix_relative` normalizes the recorded root path
    // to a safe relative POSIX path, so capture never emits `unsafe_skill_path` (that reason exists for
    // externally-supplied carriers, enforced at the import gate).

    // Dedup + recompute the verdict under the pinned worst-wins precedence.
    reasons.sort();
    reasons.dedup();
    let reason_refs: Vec<&str> = reasons.iter().map(String::as_str).collect();
    let verdict = expected_verdict(&reason_refs);

    let declared_dependencies = json!({
        "packages": packages,
        "services": services,
        "skills": skills,
    });

    Ok(json!({
        "schema": CARRIER_SCHEMA,
        "root": {"name": name, "path": resolved.root_path},
        "verdict": verdict,
        "reason_codes": reasons,
        "coverage": {
            "front_matter": front_matter,
            "body_text": body_text,
            "scripts": scripts,
            "lockfiles": lockfiles,
            "transitive_traversal": transitive_traversal,
        },
        "declared_dependencies": declared_dependencies,
        "reachable_dependencies": reachable,
        "signals": [],
        "non_claims": NON_CLAIMS,
    }))
}

/// Read a channel (`packages`/`services`/`skills`) as a list of objects from `dependencies`.
fn channel_list(deps: Option<&Value>, channel: &str) -> Vec<Value> {
    deps.and_then(|d| d.get(channel))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|item| match item {
                    Value::String(s) => json!({"name": s}),
                    other => other.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `true` when a dependency object declares `field` as a present, non-empty value (a number counts).
fn field_declared(item: &Value, field: &str) -> bool {
    match item.get(field) {
        None | Some(Value::Null) => false,
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

fn dir_has_scripts(dir: &Path) -> bool {
    if dir.join("scripts").is_dir() {
        return true;
    }
    let script_exts = ["sh", "py", "js", "ts", "rb", "ps1"];
    fs::read_dir(dir)
        .ok()
        .map(|entries| {
            entries.filter_map(|e| e.ok()).any(|e| {
                e.path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| script_exts.contains(&x))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&p, body).unwrap();
        p
    }

    fn capture_at(path: &Path) -> Value {
        let carrier = capture_carrier(path).unwrap();
        validate_carrier(&carrier).expect("captured carrier must self-validate");
        carrier
    }

    #[test]
    fn captures_bare_skill_as_complete_no_dependencies() {
        let dir = tempfile::tempdir().unwrap();
        let md = write(
            dir.path(),
            "SKILL.md",
            "---\nname: greeter\n---\nGreet the user politely.\n",
        );
        let carrier = capture_at(&md);
        assert_eq!(carrier["verdict"], "review_complete");
        assert_eq!(carrier["root"]["name"], "greeter");
        assert_eq!(carrier["coverage"]["front_matter"], "present");
        assert_eq!(carrier["coverage"]["body_text"], "present");
        assert_eq!(carrier["coverage"]["lockfiles"], "not_applicable");
        assert_eq!(
            carrier["coverage"]["transitive_traversal"],
            "not_applicable"
        );
        assert_eq!(carrier["reason_codes"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn package_without_lockfile_degrades_to_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        let md = write(
            dir.path(),
            "SKILL.md",
            "---\nname: fetcher\ndependencies:\n  packages:\n    - name: requests\n      version: 2.0\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        assert_eq!(carrier["verdict"], "review_incomplete");
        assert_eq!(carrier["coverage"]["lockfiles"], "not_present");
        let reasons: Vec<&str> = carrier["reason_codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(reasons.contains(&"missing_lockfile_evidence"));
    }

    #[test]
    fn package_with_lockfile_and_version_is_complete() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "requirements.txt", "requests==2.0\n");
        write(
            dir.path(),
            "SKILL.md",
            "---\nname: fetcher\ndependencies:\n  packages:\n    - name: requests\n      version: 2.0\n---\nBody.\n",
        );
        let carrier = capture_at(&dir.path().join("SKILL.md"));
        assert_eq!(carrier["verdict"], "review_complete");
        assert_eq!(carrier["coverage"]["lockfiles"], "present");
    }

    #[test]
    fn package_missing_version_degrades() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "requirements.txt", "requests\n");
        let md = write(
            dir.path(),
            "SKILL.md",
            "---\nname: fetcher\ndependencies:\n  packages:\n    - name: requests\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        let reasons: Vec<&str> = carrier["reason_codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(reasons.contains(&"missing_package_version"));
        assert_eq!(carrier["verdict"], "review_incomplete");
    }

    #[test]
    fn service_without_endpoint_degrades() {
        let dir = tempfile::tempdir().unwrap();
        let md = write(
            dir.path(),
            "SKILL.md",
            "---\nname: caller\ndependencies:\n  services:\n    - name: payments\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        let reasons: Vec<&str> = carrier["reason_codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(reasons.contains(&"missing_service_endpoint"));
    }

    #[test]
    fn unresolved_reused_skill_flags_traversal_not_retained() {
        let dir = tempfile::tempdir().unwrap();
        let md = write(
            dir.path(),
            "SKILL.md",
            "---\nname: parent\ndependencies:\n  skills:\n    - name: child\n      path: nonexistent/child\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        assert_eq!(carrier["coverage"]["transitive_traversal"], "not_present");
        let reasons: Vec<&str> = carrier["reason_codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(reasons.contains(&"traversal_not_retained"));
    }

    #[test]
    fn resolved_reused_skill_records_reachable_and_traverses() {
        let dir = tempfile::tempdir().unwrap();
        write(
            dir.path(),
            "child/SKILL.md",
            "---\nname: child\n---\nChild.\n",
        );
        let md = write(
            dir.path(),
            "SKILL.md",
            "---\nname: parent\ndependencies:\n  skills:\n    - name: child\n      path: child/SKILL.md\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        assert_eq!(carrier["coverage"]["transitive_traversal"], "present");
        assert_eq!(carrier["verdict"], "review_complete");
        assert_eq!(
            carrier["reachable_dependencies"].as_array().unwrap().len(),
            1
        );
    }

    #[test]
    fn scripts_directory_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "scripts/run.sh", "echo hi\n");
        write(
            dir.path(),
            "SKILL.md",
            "---\nname: withscripts\n---\nBody.\n",
        );
        let carrier = capture_at(&dir.path().join("SKILL.md"));
        assert_eq!(carrier["coverage"]["scripts"], "present");
    }

    #[test]
    fn directory_root_finds_skill_md() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "SKILL.md", "---\nname: dirskill\n---\nBody.\n");
        let carrier = capture_at(dir.path());
        assert_eq!(carrier["root"]["name"], "dirskill");
    }

    #[test]
    fn missing_front_matter_name_falls_back_to_filename_and_flags_coverage() {
        let dir = tempfile::tempdir().unwrap();
        let md = write(dir.path(), "notes.md", "Just a body, no front matter.\n");
        let carrier = capture_at(&md);
        assert_eq!(carrier["coverage"]["front_matter"], "not_present");
        assert_eq!(carrier["root"]["name"], "notes");
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_capture_a_symlinked_root() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let real = write(dir.path(), "real/SKILL.md", "---\nname: real\n---\nBody.\n");
        let link = dir.path().join("link.md");
        symlink(&real, &link).unwrap();
        let err = capture_carrier(&link).unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn refuses_a_symlinked_skill_md_inside_a_real_directory() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let target = write(
            dir.path(),
            "elsewhere/payload.md",
            "---\nname: p\n---\nBody.\n",
        );
        // A real directory whose SKILL.md is a symlink to content outside it.
        std::fs::create_dir_all(dir.path().join("skill")).unwrap();
        symlink(&target, dir.path().join("skill/SKILL.md")).unwrap();
        let err = capture_carrier(&dir.path().join("skill")).unwrap_err();
        assert!(err.to_string().contains("symlink"));
    }

    #[cfg(unix)]
    #[test]
    fn reused_skill_escaping_the_tree_via_symlink_is_not_traversed() {
        use std::os::unix::fs::symlink;
        let tree = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        // A real reused-skill target that lives OUTSIDE the reviewed tree, reached by a symlink.
        write(
            outside.path(),
            "real_child/SKILL.md",
            "---\nname: child\n---\nChild.\n",
        );
        symlink(outside.path().join("real_child"), tree.path().join("child")).unwrap();
        let md = write(
            tree.path(),
            "SKILL.md",
            "---\nname: parent\ndependencies:\n  skills:\n    - name: child\n      path: child/SKILL.md\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        // The reused skill escapes the tree through the symlink, so traversal is not retained.
        assert_eq!(carrier["coverage"]["transitive_traversal"], "not_present");
        let reasons: Vec<&str> = carrier["reason_codes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect();
        assert!(reasons.contains(&"traversal_not_retained"));
    }

    #[cfg(unix)]
    #[test]
    fn reused_skill_via_internal_symlink_still_traverses() {
        use std::os::unix::fs::symlink;
        let tree = tempfile::tempdir().unwrap();
        // An internal symlink that stays inside the tree is benign: traversal still succeeds.
        write(
            tree.path(),
            "real_child/SKILL.md",
            "---\nname: child\n---\nChild.\n",
        );
        symlink(tree.path().join("real_child"), tree.path().join("child")).unwrap();
        let md = write(
            tree.path(),
            "SKILL.md",
            "---\nname: parent\ndependencies:\n  skills:\n    - name: child\n      path: child/SKILL.md\n---\nBody.\n",
        );
        let carrier = capture_at(&md);
        assert_eq!(carrier["coverage"]["transitive_traversal"], "present");
        assert_eq!(carrier["verdict"], "review_complete");
    }
}
