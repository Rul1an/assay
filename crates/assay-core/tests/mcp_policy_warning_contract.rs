//! Subprocess warning/ordering contract for the full-policy parser (#2387).
//!
//! Shape: parent spawns `current_exe()` with `ASSAY_MCP_POLICY_WARNING_CHILD=1`
//! and `--ignored --exact` flags. The child installs a no-timestamp tracing
//! subscriber to stderr, then parses a V1 fixture with an unknown field under
//! `tools:` (non-flattened, so `serde_ignored` fires) twice. The parent reads
//! the child's stderr and asserts:
//!
//! 1. Unknown-field tracing warning precedes the deprecation `eprintln!`.
//! 2. "DEPRECATED" appears exactly once (OnceLock single-fire over two parses).
//! 3. The child succeeds.
#![allow(unsafe_code)]

fn run_child() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = dir.path().join("v1_unknown_under_tools.yaml");
    // Put the unknown field under `tools:` so serde_ignored catches it;
    // root-level unknowns are absorbed by the flattened ToolTaxonomy.
    std::fs::write(
        &fixture,
        b"version: \"1.0\"\ntools:\n  allow:\n    - read_file\n  unknown_field_xyz: true\nconstraints: []\n",
    )
    .unwrap();

    unsafe { std::env::remove_var("ASSAY_STRICT_DEPRECATIONS") };

    // Install a global tracing subscriber so `tracing::warn!` in from_file
    // reaches stderr. This is a fresh child process so no prior subscriber.
    let subscriber = tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .without_time()
        .with_max_level(tracing::Level::WARN)
        .finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("child must own the global subscriber");

    // First parse: unknown-field warning then deprecation warning
    let _p1 = assay_core::mcp::policy::McpPolicy::from_file(&fixture).unwrap();

    // Second parse: deprecation must NOT repeat (OnceLock)
    let _p2 = assay_core::mcp::policy::McpPolicy::from_file(&fixture).unwrap();
}

#[test]
#[ignore] // only run as a subprocess child
fn deprecation_warning_fires_exactly_once() {
    if std::env::var("ASSAY_MCP_POLICY_WARNING_CHILD").is_ok() {
        run_child();
    }
    // If reached without the env var, skip (--ignored prevents this normally).
}

#[test]
fn warning_ordering_and_single_fire_contract() {
    // Parent: spawn self as the ignored exact child
    let exe = std::env::current_exe().unwrap();
    let output = std::process::Command::new(&exe)
        .arg("--ignored")
        .arg("--exact")
        .arg("deprecation_warning_fires_exactly_once")
        .arg("--nocapture")
        .env("ASSAY_MCP_POLICY_WARNING_CHILD", "1")
        .output()
        .expect("spawn child");

    assert!(
        output.status.success(),
        "child failed with code {:?}:\nstdout={}\nstderr={}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Unknown-field warning (tracing::warn!) must contain the field name
    let has_unknown = stderr.contains("unknown_field_xyz") || stderr.contains("Unknown fields");
    assert!(
        has_unknown,
        "unknown-field warning not found in child stderr:\n{stderr}"
    );

    // Deprecation warning (eprintln!) must be present
    assert!(
        stderr.contains("DEPRECATED"),
        "deprecation warning not found in child stderr:\n{stderr}"
    );

    // Migration instruction must be present
    assert!(
        stderr.contains("assay policy migrate"),
        "migration instruction not found in child stderr:\n{stderr}"
    );

    // Ordering: unknown-field warning must precede deprecation
    let unknown_pos = stderr
        .find("unknown_field_xyz")
        .or_else(|| stderr.find("Unknown fields"))
        .unwrap();
    let deprecation_pos = stderr.find("DEPRECATED").unwrap();
    assert!(
        unknown_pos < deprecation_pos,
        "unknown-field warning (byte {unknown_pos}) must precede deprecation \
         warning (byte {deprecation_pos}):\n{stderr}"
    );

    // "DEPRECATED" must appear exactly once (OnceLock fires once, two parses)
    let deprecated_count = stderr.matches("DEPRECATED").count();
    assert_eq!(
        deprecated_count, 1,
        "DEPRECATED must appear exactly 1 time, found {deprecated_count}:\n{stderr}"
    );
}
