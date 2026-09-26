//! Integration tests for capture-side redaction (ADR-034 Phase 1).
//!
//! The bar: no raw planted secret survives in the serialized bundle, the fail-closed sweep catches a
//! missed funnel, and environment variable VALUES are never serialized.

use assay_runner_core::{RedactMode, Redactor, RunSpec, RunnerSpikeArchive};

// Secret SHAPES assembled from fragments so the repo secret scanner does not flag this test file.
fn gh_token() -> String {
    format!("gh{}_{}{}", "p", "0123456789abcdef".repeat(2), "0123")
}

fn redactor() -> Redactor {
    Redactor::new(RedactMode::ShapeAndFlag, b"installation-key", Vec::new())
}

#[test]
fn no_raw_planted_secret_in_serialized_archive() {
    let tok = gh_token();
    let mut archive = RunnerSpikeArchive::empty("run_redact", "linux");

    // A planted argv secret in a run_started event (the highest-risk vector).
    archive.events_ndjson = format!(
        "{{\"schema\":\"assay.runner.run_event.v0\",\"run_id\":\"run_redact\",\"seq\":0,\
         \"type\":\"run_started\",\"command\":[\"agent\",\"--token\",\"{tok}\"],\
         \"env_keys\":[\"PATH\"]}}\n"
    )
    .into_bytes();
    // A planted secret in a kernel-observed filesystem path.
    archive
        .capability_surface
        .add_filesystem_path(format!("/tmp/cfg/{tok}.json"));
    // And a short, non-shape-matchable credential after a flag (only flag-aware catches this).
    let pw = format!("{}{}", "hunter2", "short");
    archive
        .capability_surface
        .add_process_exec(format!("agent --password {pw}"));

    let r = redactor();
    let tally = archive.redact_in_place(&r);
    assert!(tally.total >= 2, "expected redactions, got {}", tally.total);

    // The fail-closed sweep passes once redaction has run.
    archive
        .assert_no_unredacted(&r)
        .expect("no unredacted secret should remain after redaction");

    // The raw token is gone from every in-memory field that gets serialized.
    assert!(!String::from_utf8_lossy(&archive.events_ndjson).contains(&tok));
    for p in &archive.capability_surface.filesystem_paths {
        assert!(!p.contains(&tok), "raw token leaked in {p}");
    }
    // The flag-aware short password is gone too.
    let execs = archive
        .capability_surface
        .process_execs
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(!execs.contains(&pw));
}

#[test]
fn url_userinfo_in_endpoint_is_redacted_preserving_host() {
    let pw = format!("s3cr3t{}", "pass");
    let mut archive = RunnerSpikeArchive::empty("run_url", "linux");
    archive
        .capability_surface
        .add_network_endpoint(format!("postgres://svc:{pw}@db.internal:5432/app"));

    let r = redactor();
    let tally = archive.redact_in_place(&r);
    assert!(tally.total >= 1);
    let eps: Vec<String> = archive
        .capability_surface
        .network_endpoints
        .iter()
        .cloned()
        .collect();
    let joined = eps.join(" ");
    assert!(!joined.contains(&pw), "raw password leaked: {joined}");
    assert!(
        joined.contains("@db.internal:5432/app"),
        "host should be preserved: {joined}"
    );
    archive
        .assert_no_unredacted(&r)
        .expect("no unredacted secret after redaction");
}

#[test]
fn fail_closed_sweep_catches_a_missed_funnel() {
    let tok = gh_token();
    let mut archive = RunnerSpikeArchive::empty("run_leak", "linux");
    // Simulate a capture funnel that was NOT redacted: a raw shape secret in a surface field.
    archive
        .capability_surface
        .add_filesystem_path(format!("/tmp/{tok}.key"));

    let r = redactor();
    // No redact_in_place call -> the sweep must fail closed rather than allow a bundle.
    let err = archive.assert_no_unredacted(&r).unwrap_err();
    assert!(
        format!("{err}").contains("github-token"),
        "expected an unredacted-secret error, got: {err}"
    );
}

#[test]
fn redaction_is_idempotent_and_leaves_clean_content_unchanged() {
    let mut archive = RunnerSpikeArchive::empty("run_clean", "linux");
    archive.events_ndjson =
        b"{\"type\":\"run_started\",\"command\":[\"agent\",\"--verbose\"]}\n".to_vec();
    archive
        .capability_surface
        .add_filesystem_path("/workspace/src/main.rs".to_string());
    let before = archive.clone();

    let r = redactor();
    let tally = archive.redact_in_place(&r);
    assert_eq!(tally.total, 0, "clean content should not be redacted");
    assert_eq!(
        archive.events_ndjson, before.events_ndjson,
        "a clean line must stay byte-identical"
    );
    assert_eq!(
        archive.capability_surface, before.capability_surface,
        "a clean surface must be unchanged"
    );
}

#[test]
fn env_variable_values_are_never_serialized() {
    let secret = gh_token();
    let spec = RunSpec::new(vec!["true".to_string()])
        .with_run_id("run_env")
        .with_env("MY_SECRET", secret.clone());
    let mut archive = RunnerSpikeArchive::empty("run_env", "linux");
    spec.append_run_started(&mut archive, 0, std::time::Duration::ZERO)
        .expect("append run_started");

    let events = String::from_utf8_lossy(&archive.events_ndjson);
    // The env KEY is recorded (so a reviewer sees which vars were set)...
    assert!(events.contains("MY_SECRET"), "env key should be recorded");
    // ...but the env VALUE must never appear, redaction or not.
    assert!(
        !events.contains(&secret),
        "env value must never be serialized"
    );
}

/// A stateless installation token with the minimal `{"alg":"RS256"}` header, assembled from
/// fragments so this file carries no scannable token.
fn short_header_stateless_parts() -> (String, String, String, String) {
    let app = format!("gh{}_4242424_", "s");
    let header = format!("ey{}", "JhbGciOiJSUzI1NiJ9");
    let payload = format!("ey{}", "Jpc3MiOiJwcm9iZS1ub3QtYS1zZWNyZXQifQ");
    let signature = format!("{}{}", "UFJPQkUtRkFLRS1TSUdOQVRVUkU", "tbm90LXJlYWw-");
    (app, header, payload, signature)
}

/// `(label, secret, suffix)`: each secret, written into a path followed by `suffix`, must be redacted
/// to the same placeholder it gets as an event value, and `suffix` must survive.
fn github_token_sink_cases() -> Vec<(&'static str, String, &'static str)> {
    let (app, header, payload, signature) = short_header_stateless_parts();
    let opaque_with_underscore = format!("gh{}_{}_{}", "p", "A".repeat(12), "B".repeat(23));
    vec![
        // Truncated stateless fragments: every cut still redacts what is there.
        (
            "stateless cut in header",
            format!("{app}{}", &header[..8]),
            "/cfg.json",
        ),
        (
            "stateless cut after header",
            format!("{app}{header}"),
            "/cfg.json",
        ),
        (
            "stateless header then extension",
            format!("{app}{header}"),
            ".json",
        ),
        (
            "stateless cut in payload",
            format!("{app}{header}.{}", &payload[..5]),
            "/cfg.json",
        ),
        // The reviewed counterexample, verbatim.
        (
            "stateless cut in payload (review)",
            format!("{app}{header}.ey{}", "Jab"),
            "/cfg.json",
        ),
        (
            "stateless cut after payload",
            format!("{app}{header}.{payload}"),
            "/cfg.json",
        ),
        (
            "stateless cut in signature",
            format!("{app}{header}.{payload}.{}", &signature[..4]),
            "/cfg.json",
        ),
        (
            "stateless whole, long extension",
            format!("{app}{header}.{payload}.{signature}"),
            ".backup-2026-09-26",
        ),
        // Opaque tokens: a dotted suffix is never part of the token, whatever the prefix.
        (
            "opaque with underscore, long extension",
            opaque_with_underscore.clone(),
            ".backup-2026-09-26",
        ),
        (
            "opaque with underscore, json",
            opaque_with_underscore,
            ".json",
        ),
        (
            "opaque ghs, long extension",
            format!("gh{}_{}", "s", "C".repeat(36)),
            ".backup-2026-09-26",
        ),
        (
            "opaque gho, tar.gz",
            format!("gh{}_{}", "o", "D".repeat(40)),
            ".tar.gz",
        ),
    ]
}

/// Plants `secret` as an event value and in a path, then checks the sweep refuses it before
/// redaction, accepts the result after, and that path and value carry one placeholder.
fn check_github_token_sink_case(secret: &str, suffix: &str) -> Result<(), String> {
    let r = redactor();
    let mut archive = RunnerSpikeArchive::empty("run_gh_sink", "linux");
    archive.events_ndjson =
        format!("{{\"type\":\"probe.env\",\"seq\":0,\"value\":\"{secret}\"}}\n").into_bytes();
    archive
        .capability_surface
        .add_filesystem_path(format!("/tmp/probe/{secret}{suffix}"));

    match archive.assert_no_unredacted(&r) {
        Ok(()) => return Err("sweep passed the unredacted secret".into()),
        Err(e) if !format!("{e}").contains("github-token") => {
            return Err(format!("sweep refused for another rule: {e}"))
        }
        Err(_) => {}
    }

    let tally = archive.redact_in_place(&r);
    if tally.by_rule.get("github-token") != Some(&2) {
        return Err(format!("github-token count {:?}, want 2", tally.by_rule));
    }
    archive
        .assert_no_unredacted(&r)
        .map_err(|e| format!("sweep refused after redaction: {e}"))?;

    let events = String::from_utf8_lossy(&archive.events_ndjson).into_owned();
    let placeholder = events
        .split("\"value\":\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .unwrap_or_default()
        .to_string();
    if !(placeholder.starts_with("<redacted:github-token:") && placeholder.ends_with('>')) {
        return Err(format!("event value not wholly redacted: {placeholder}"));
    }
    let paths: Vec<String> = archive
        .capability_surface
        .filesystem_paths
        .iter()
        .cloned()
        .collect();
    let want = format!("/tmp/probe/{placeholder}{suffix}");
    if paths != [want.clone()] {
        return Err(format!("path {paths:?}, want [{want}]"));
    }
    Ok(())
}

#[test]
fn github_token_fragments_redact_to_one_placeholder_and_keep_their_suffix() {
    let failures: Vec<String> = github_token_sink_cases()
        .into_iter()
        .filter_map(|(label, secret, suffix)| {
            check_github_token_sink_case(&secret, suffix)
                .err()
                .map(|e| format!("{label}: {e}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
