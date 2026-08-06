//! A discovery method a policy can name must not be accepted and then ignored.
//!
//! `DiscoveryMethod` derives `Deserialize` and `JsonSchema`, so `methods: ["network"]` is a valid
//! policy. `assay mcp discover` matched two variants and caught the other three with
//! `_ => {} // not implemented yet`: the run scanned nothing for them, printed nothing, and exited
//! 0. An operator reads that as "discovery found nothing" rather than "discovery never ran" —
//! which is the class #2039 and #2032 are about, on a policy surface rather than a CLI flag.

use std::process::Command;

fn policy_with(methods: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("policy.yaml");
    std::fs::write(
        &path,
        format!("version: \"1\"\ntools:\n  allow: [\"read\"]\ndiscovery:\n  enabled: true\n  methods: {methods}\n"),
    )
    .expect("write policy");
    (dir, path)
}

fn discover(policy: &std::path::Path) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["mcp", "discover", "--policy"])
        .arg(policy)
        .output()
        .expect("failed to run assay");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn an_unimplemented_method_says_it_scanned_nothing() {
    let (_dir, policy) = policy_with(r#"["network", "dns", "well_known"]"#);
    let (ok, stderr) = discover(&policy);
    assert!(ok, "the run must still succeed: {stderr}");
    for method in ["Network", "Dns", "WellKnown"] {
        assert!(
            stderr.contains(method),
            "no warning for {method}, so a policy naming it looks like it ran: {stderr}"
        );
    }
    assert!(stderr.contains("not implemented"));
}

/// A warning, not a refusal.
///
/// A policy may list an unimplemented method beside one that works, and failing the whole run would
/// punish the working half. The implemented method must still produce its scan.
#[test]
fn an_unimplemented_method_does_not_fail_the_run() {
    let (_dir, policy) = policy_with(r#"["config_files", "network"]"#);
    let (ok, stderr) = discover(&policy);
    assert!(
        ok,
        "an unimplemented method must not fail the run: {stderr}"
    );
    assert!(stderr.contains("Network"));
}

/// The implemented methods stay silent. A warning on every run would train an operator to ignore it.
#[test]
fn implemented_methods_warn_about_nothing() {
    let (_dir, policy) = policy_with(r#"["config_files", "processes"]"#);
    let (ok, stderr) = discover(&policy);
    assert!(ok, "{stderr}");
    assert!(
        !stderr.contains("not implemented"),
        "warned about a method that is implemented: {stderr}"
    );
}
