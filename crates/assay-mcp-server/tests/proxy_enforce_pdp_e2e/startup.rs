use crate::support::*;

// --- startup failures (non-zero exit; both inputs required in enforcing mode) ----------------------

fn assert_machine_startup_failure(extra: &[&str], reason_code: &str, next_step: &str) {
    let output = run_startup_output(extra);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "startup failure polluted MCP stdout"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
    let events: Vec<serde_json::Value> = stderr
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .filter(|event: &serde_json::Value| event["event"] == "startup_failure")
        .collect();
    assert_eq!(
        events.len(),
        1,
        "stderr did not carry one startup_failure: {stderr}"
    );
    assert_eq!(events[0]["reason_code"], reason_code);
    assert_eq!(events[0]["next_step"], next_step);
    // Path-freeness is a property of the object, not of the line it sits on: `serde_json` escapes
    // a newline, so a path admitted into this object would still parse as one event and slip past
    // the count above. Pinning the whole key set is what refuses the field in the first place.
    let carried: std::collections::BTreeSet<&str> = events[0]
        .as_object()
        .expect("the machine event must be a JSON object")
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        carried,
        ["event", "next_step", "reason_code"]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>(),
        "the machine event grew a field; caller-controlled input must not enter it: {stderr}"
    );
    assert!(
        !stderr.contains("\"event\":\"proxy_start\""),
        "failed startup claimed that the proxy started: {stderr}"
    );
}

#[test]
fn missing_enforce_policy_flag_fails_startup() {
    let baseline = approved_baseline_path();
    let status = run_startup(&["--declared-mcp-manifest", baseline.to_str().unwrap()]);
    assert!(
        !status.success(),
        "missing --enforce-policy must fail startup"
    );
}

#[test]
fn missing_declared_manifest_flag_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let status = run_startup(&["--enforce-policy", policy.to_str().unwrap()]);
    assert!(
        !status.success(),
        "missing --declared-mcp-manifest must fail startup in enforcing mode"
    );
}

#[test]
fn missing_policy_file_emits_one_machine_readable_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope.yaml");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            missing.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
        ],
        "proxy_enforce_policy_invalid",
        "Check --enforce-policy and retry proxy-enforce.",
    );
}

#[test]
fn malformed_policy_file_emits_policy_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", "caller: [");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
        ],
        "proxy_enforce_policy_invalid",
        "Check --enforce-policy and retry proxy-enforce.",
    );
}

#[test]
fn malformed_manifest_emits_manifest_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let malformed = write_file(dir.path(), "malformed.json", "{");

    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            malformed.to_str().unwrap(),
        ],
        "proxy_declared_manifest_invalid",
        "Check --declared-mcp-manifest and retry proxy-enforce.",
    );
}

#[test]
fn missing_caller_id_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", "allowances: []\n");
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "policy without caller.id must fail startup"
    );
}

#[test]
fn missing_declared_manifest_file_emits_manifest_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let missing = dir.path().join("nope.json");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            missing.to_str().unwrap(),
        ],
        "proxy_declared_manifest_invalid",
        "Check --declared-mcp-manifest and retry proxy-enforce.",
    );
}

#[test]
fn wrong_schema_declared_manifest_fails_startup() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let baseline = write_file(
        dir.path(),
        "baseline.json",
        r#"{"schema":"assay.mcp_manifest_observed.v0","tools":[{"name":"t","tool_digest":"sha256:abc"}]}"#,
    );
    let status = run_startup(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        baseline.to_str().unwrap(),
    ]);
    assert!(
        !status.success(),
        "a wrong-schema baseline must fail startup"
    );
}

#[test]
fn establish_budget_zero_emits_budget_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
            "--manifest-establish-budget-ms",
            "0",
        ],
        "proxy_establish_budget_invalid",
        "Set --manifest-establish-budget-ms above 0 and retry proxy-enforce.",
    );
}

// --- line uniqueness under a newline-carrying path (#2436) -----------------------------------
//
// The machine emitter is already path-free and already emits once. What was not hardened is the
// HUMAN chain that may follow it: `anyhow`'s rendering interpolates the rejected path, a Unix path
// may contain a newline, and an unprefixed continuation line is indistinguishable from a machine
// event to a line-oriented consumer. These rows drive that consumer.

/// The operator-channel prefix, written out rather than imported: `main.rs` is a binary, and a
/// contract test that computed this from the code under test could not see it change.
const HUMAN_ERROR_PREFIX: &str = "assay-mcp-server: ";

/// A complete machine event, shaped exactly as a caller would embed one in a path.
#[cfg(unix)]
const INJECTED_STARTUP_FAILURE: &str = concat!(
    r#"{"event":"startup_failure","reason_code":"injected_not_a_product_reason","#,
    r#""next_step":"injected"}"#
);

/// Unix-only because the fixture is the path itself: Windows forbids a newline in a file name, so
/// there is no such path to construct rather than a behaviour that differs there.
#[cfg(unix)]
fn newline_injected_path(dir: &std::path::Path, stem: &str) -> std::path::PathBuf {
    dir.join(format!("{stem}\n{INJECTED_STARTUP_FAILURE}"))
}

#[cfg(unix)]
#[test]
fn newline_in_missing_policy_path_keeps_one_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let missing = newline_injected_path(dir.path(), "nope.yaml");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            missing.to_str().unwrap(),
            "--declared-mcp-manifest",
            approved_baseline_path().to_str().unwrap(),
        ],
        "proxy_enforce_policy_invalid",
        "Check --enforce-policy and retry proxy-enforce.",
    );
}

#[cfg(unix)]
#[test]
fn newline_in_missing_declared_manifest_path_keeps_one_startup_failure() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let missing = newline_injected_path(dir.path(), "nope.json");
    assert_machine_startup_failure(
        &[
            "--enforce-policy",
            policy.to_str().unwrap(),
            "--declared-mcp-manifest",
            missing.to_str().unwrap(),
        ],
        "proxy_declared_manifest_invalid",
        "Check --declared-mcp-manifest and retry proxy-enforce.",
    );
}

/// Platform-neutral preservation row for a startup failure that carries no path at all.
///
/// Uniqueness alone is satisfiable by deleting the human chain, which would take the operator's
/// diagnosis with it. This pins both halves: the one machine event stays, and the chain still
/// reaches stderr — every line of it behind a prefix that cannot open a JSON document.
#[test]
fn path_free_startup_failure_keeps_its_event_and_a_prefixed_human_chain() {
    let dir = tempfile::tempdir().unwrap();
    let policy = write_file(dir.path(), "enforce.yaml", ALLOW_ACME);
    let output = run_startup_output(&[
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        approved_baseline_path().to_str().unwrap(),
        "--manifest-establish-budget-ms",
        "0",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(
        output.stdout.is_empty(),
        "startup failure polluted MCP stdout"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr must be UTF-8");
    let machine_events = stderr
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["event"] == "startup_failure")
        .count();
    assert_eq!(
        machine_events, 1,
        "the path-free failure must still carry exactly one machine event: {stderr}"
    );

    let chain: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with(HUMAN_ERROR_PREFIX))
        .collect();
    assert!(
        !chain.is_empty(),
        "the human diagnosis was removed rather than prefixed: {stderr}"
    );
    assert!(
        chain
            .iter()
            .any(|line| line.contains("--manifest-establish-budget-ms")),
        "the prefixed chain must still name the rejected input: {stderr}"
    );
}

/// Runs the startup with a deadline: a proxy that starts instead of failing is killed and reported,
/// so the test fails rather than hangs.
fn startup_within(extra: &[&str], deadline: std::time::Duration) -> (Option<i32>, String) {
    let mut args = upstream_args();
    args.extend(extra.iter().map(|e| (*e).to_string()));
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_assay-mcp-server"))
        .args(&args)
        .env("ASSAY_LOG", "off")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    let started = std::time::Instant::now();
    while child.try_wait().expect("poll").is_none() {
        if started.elapsed() > deadline {
            child.kill().ok();
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    let output = child.wait_with_output().expect("output");
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A policy key the proxy does not act on fails startup with the policy's reason code, and the
/// human line names the key. The key's text reaches stderr escaped and on a prefixed line.
#[test]
fn a_policy_key_the_proxy_does_not_act_on_fails_startup_and_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let body = ALLOW_ACME.replace(
        r#"{ owner: "acme", repo: "prod-app" }"#,
        "{ owner: \"acme\", repo: \"prod-app\", read_only: true, \"\\e]52;c;cHduZWQ=\\a\\n{\\\"event\\\":\\\"startup_failure\\\"}\": 1 }",
    );
    assert_ne!(body, ALLOW_ACME);
    let policy = write_file(dir.path(), "enforce.yaml", &body);
    let baseline = approved_baseline_path();
    let extra = [
        "--enforce-policy",
        policy.to_str().unwrap(),
        "--declared-mcp-manifest",
        baseline.to_str().unwrap(),
    ];
    let (code, stderr) = startup_within(&extra, std::time::Duration::from_secs(20));
    assert_eq!(code, Some(1), "the proxy must not start: {stderr}");
    assert!(
        stderr.contains("\"allowances.0.targets.0.read_only\""),
        "the key must be named: {stderr}"
    );
    let machine: Vec<&str> = stderr
        .lines()
        .filter(|line| line.starts_with('{'))
        .collect();
    assert_eq!(machine.len(), 1, "one machine event only: {stderr}");
    assert!(
        machine[0].contains("\"reason_code\":\"proxy_enforce_policy_invalid\""),
        "{stderr}"
    );
    for line in stderr.lines().filter(|line| !line.starts_with('{')) {
        assert!(
            line.starts_with("assay-mcp-server: "),
            "a human line without the prefix: {line:?}"
        );
    }
    for (index, byte) in stderr.bytes().enumerate() {
        assert!(
            (byte >= 0x20 && byte != 0x7f) || byte == b'\n',
            "stderr byte {index} is the control byte {byte:#04x}: {stderr}"
        );
    }

    // Control: the same policy without those keys still passes the policy stage, failing only on
    // what follows it in this harness, or not at all.
    let clean = write_file(dir.path(), "clean.yaml", ALLOW_ACME);
    let (_, clean_stderr) = startup_within(
        &[
            "--enforce-policy",
            clean.to_str().unwrap(),
            "--declared-mcp-manifest",
            baseline.to_str().unwrap(),
        ],
        std::time::Duration::from_secs(20),
    );
    assert!(
        !clean_stderr.contains("proxy_enforce_policy_invalid"),
        "the clean policy was refused: {clean_stderr}"
    );
}
