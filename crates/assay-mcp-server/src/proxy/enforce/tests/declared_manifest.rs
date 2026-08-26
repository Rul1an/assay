//! #2654 Slice A: strict declared-v0 manifest truth at `--declared-mcp-manifest` startup.
//!
//! Two jobs. FREEZE: the committed declared fixtures that load today must keep loading, byte
//! unchanged, so the strictness added here cannot be paid for by breaking a valid operator baseline.
//! RED: the members the loader used to ignore — unknown members, a wrong or absent canonicalization
//! id, a malformed `sha256:` digest, and a `manifest_digest` that does not recompute — must fail
//! before the proxy starts, because a baseline that is not what it says it is cannot be an approval.
use super::*;
use serde_json::Value;

/// Repository-root-relative fixture, resolved from this crate's manifest dir.
fn repo_path(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(rel)
}

/// Every committed declared baseline a reader could legitimately hand to `--declared-mcp-manifest`.
fn committed_declared_fixtures() -> Vec<std::path::PathBuf> {
    [
        "crates/assay-mcp-server/tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline.json",
        "crates/assay-mcp-server/tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline_readonly_annotation.json",
        "examples/privileged-action-gate/baseline-approved.json",
        "examples/privileged-action-gate/baseline-approved-readonly.json",
    ]
    .iter()
    .map(|r| repo_path(r))
    .collect()
}

/// Load a declared manifest from an in-memory value, through the production startup path.
fn load_value(v: &Value) -> Result<DeclaredManifest> {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(serde_json::to_string_pretty(v).unwrap().as_bytes())
        .unwrap();
    load_declared_manifest(f.path())
}

/// The canonical valid baseline every RED case below mutates exactly one thing away from.
fn valid_declared() -> Value {
    let p = repo_path(
        "crates/assay-mcp-server/tests/fixtures/mcp_manifest_drift/declared_per_tool_baseline.json",
    );
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
}

// ---------------------------------------------------------------- FREEZE

#[test]
fn committed_declared_fixtures_still_load_unchanged() {
    for p in committed_declared_fixtures() {
        let m = load_declared_manifest(&p)
            .unwrap_or_else(|e| panic!("committed fixture {} must load: {e:#}", p.display()));
        assert_eq!(m.schema, "assay.declared_mcp_manifest.v0");
        assert!(
            !m.tools.is_empty(),
            "committed fixture {} must declare tools",
            p.display()
        );
    }
}

#[test]
fn valid_declared_control_loads() {
    // The no-op control for every mutation below: unmutated, it loads.
    load_value(&valid_declared()).expect("unmutated valid baseline must load");
}

// ------------------------------------------------------------------- RED

#[test]
fn unknown_top_level_member_fails_startup() {
    let mut v = valid_declared();
    v.as_object_mut()
        .unwrap()
        .insert("rogue_member".into(), Value::Bool(true));
    assert!(
        load_value(&v).is_err(),
        "an unknown top-level member must fail startup, not be silently ignored"
    );
}

#[test]
fn unknown_tool_member_fails_startup() {
    let mut v = valid_declared();
    v["tools"][0]
        .as_object_mut()
        .unwrap()
        .insert("rogue_member".into(), Value::Bool(true));
    assert!(
        load_value(&v).is_err(),
        "an unknown per-tool member must fail startup, not be silently ignored"
    );
}

#[test]
fn absent_canonicalization_fails_startup() {
    let mut v = valid_declared();
    v.as_object_mut().unwrap().remove("canonicalization");
    assert!(
        load_value(&v).is_err(),
        "a baseline without a canonicalization id must fail startup"
    );
}

#[test]
fn wrong_canonicalization_fails_startup() {
    let mut v = valid_declared();
    v["canonicalization"] = Value::String("assay.some_other_projection.v0".into());
    assert!(
        load_value(&v).is_err(),
        "a baseline naming another canonicalization must fail startup"
    );
}

#[test]
fn short_tool_digest_fails_startup() {
    let mut v = valid_declared();
    v["tools"][0]["tool_digest"] = Value::String("sha256:abc".into());
    assert!(
        load_value(&v).is_err(),
        "a sha256: prefix with the wrong length must fail startup"
    );
}

#[test]
fn non_hex_tool_digest_fails_startup() {
    let mut v = valid_declared();
    v["tools"][0]["tool_digest"] = Value::String(format!("sha256:{}", "z".repeat(64)));
    assert!(
        load_value(&v).is_err(),
        "a sha256: digest that is not lowercase hex must fail startup"
    );
}

#[test]
fn uppercase_tool_digest_fails_startup() {
    let mut v = valid_declared();
    let d = v["tools"][0]["tool_digest"].as_str().unwrap().to_string();
    v["tools"][0]["tool_digest"] = Value::String(d.to_uppercase().replace("SHA256:", "sha256:"));
    assert!(
        load_value(&v).is_err(),
        "an uppercase-hex digest must fail startup; digests compare as exact bytes"
    );
}

#[test]
fn absent_manifest_digest_fails_startup() {
    let mut v = valid_declared();
    v.as_object_mut().unwrap().remove("manifest_digest");
    assert!(
        load_value(&v).is_err(),
        "a baseline without a manifest_digest must fail startup"
    );
}

#[test]
fn tool_digest_mutated_without_manifest_digest_fails_startup() {
    let mut v = valid_declared();
    // Flip the last hex nibble of one tool digest, leaving manifest_digest stale.
    let d = v["tools"][0]["tool_digest"].as_str().unwrap().to_string();
    let flipped = if d.ends_with('0') {
        format!("{}1", &d[..d.len() - 1])
    } else {
        format!("{}0", &d[..d.len() - 1])
    };
    v["tools"][0]["tool_digest"] = Value::String(flipped);
    assert!(
        load_value(&v).is_err(),
        "a tool digest that moves without its manifest_digest must fail startup"
    );
}

#[test]
fn manifest_digest_mismatch_fails_startup() {
    let mut v = valid_declared();
    v["manifest_digest"] = Value::String(format!("sha256:{}", "0".repeat(64)));
    assert!(
        load_value(&v).is_err(),
        "a manifest_digest that does not recompute must fail startup"
    );
}

// ------------------------------------------- behaviour that must NOT regress

#[test]
fn duplicate_tool_name_still_fails_startup() {
    // NEAR MISS: the duplicate is added AND the manifest_digest is recomputed over the duplicated
    // entries, so this document violates exactly one rule — duplicate names. Without the
    // recomputation the digest check would reject it first and the duplicate-name refusal would
    // never be the reason, which is a guard that looks pinned and is not.
    let mut v = valid_declared();
    let first = v["tools"][0].clone();
    v["tools"].as_array_mut().unwrap().push(first);
    let pairs: Vec<(String, String)> = v["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| {
            (
                t["name"].as_str().unwrap().to_string(),
                t["tool_digest"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    v["manifest_digest"] =
        Value::String(assay_mcp_server::manifest_observed::manifest_digest(&pairs));
    assert!(
        load_value(&v).is_err(),
        "duplicate declared names must remain a startup failure on their own"
    );
}

#[test]
fn wrong_schema_still_fails_startup() {
    let mut v = valid_declared();
    v["schema"] = Value::String("assay.declared_mcp_manifest.v1".into());
    assert!(
        load_value(&v).is_err(),
        "a foreign schema id must remain a startup failure"
    );
}

#[test]
fn empty_tools_still_fails_startup() {
    let mut v = valid_declared();
    v["tools"] = Value::Array(vec![]);
    assert!(
        load_value(&v).is_err(),
        "an empty tools array must remain a startup failure"
    );
}
