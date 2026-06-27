use std::borrow::Cow;

use regex::Regex;

use super::*;

// Secret SHAPES assembled from fragments at runtime, so no whole-token literal is committed and
// the repo secret scanner does not flag this test file (same pattern as the Plimsoll tests).
fn gh() -> String {
    format!("gh{}_{}{}", "p", "0123456789abcdef".repeat(2), "0123")
}

fn aws() -> String {
    format!("AK{}{}{}", "IA", "IOSFODNN7", "EXAMPLE")
}

// Weak/synthetic credential strings, assembled at runtime so the repo secret scanner does not
// flag this test file.
fn pw_short() -> String {
    format!("{}{}", "hunter2", "short")
}

fn redactor(mode: RedactMode) -> Redactor {
    Redactor::new(mode, b"installation-secret-key", Vec::new())
}

#[test]
fn clean_value_is_borrowed_unchanged() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let out = r.redact_value("filesystem_paths", "/workspace/src/main.py", &mut t);
    assert_eq!(out, "/workspace/src/main.py");
    assert!(matches!(out, Cow::Borrowed(_)));
    assert!(t.is_empty());
}

#[test]
fn shape_pass_redacts_and_never_echoes_value() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let token = gh();
    let input = format!("/tmp/cfg/{token}.json");
    let out = r.redact_value("filesystem_paths", &input, &mut t);
    assert!(out.contains("<redacted:github-token:"));
    assert!(!out.contains(&token));
    assert_eq!(t.total, 1);
    assert_eq!(t.by_rule.get("github-token"), Some(&1));
    assert_eq!(t.by_field.get("filesystem_paths"), Some(&1));
}

#[test]
fn deterministic_same_secret_same_placeholder() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let token = gh();
    let mut t = RedactionTally::default();
    let a = r.redact_value("a", &token, &mut t).into_owned();
    let b = r.redact_value("b", &token, &mut t).into_owned();
    assert_eq!(a, b);
}

#[test]
fn salt_changes_placeholder() {
    let token = gh();
    let mut t = RedactionTally::default();
    let r1 = Redactor::new(RedactMode::ShapeAndFlag, b"salt-one", Vec::new());
    let r2 = Redactor::new(RedactMode::ShapeAndFlag, b"salt-two", Vec::new());
    let a = r1.redact_value("f", &token, &mut t).into_owned();
    let b = r2.redact_value("f", &token, &mut t).into_owned();
    assert_ne!(a, b);
}

#[test]
fn idempotent_placeholder_not_rematched() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let once = r.redact_value("f", &gh(), &mut t).into_owned();
    let mut t2 = RedactionTally::default();
    let twice = r.redact_value("f", &once, &mut t2);
    assert_eq!(twice, once);
    assert!(t2.is_empty());
}

#[test]
fn argv_flag_aware_redacts_value_token() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let pw = pw_short();
    let argv = vec!["agent".to_string(), "--password".to_string(), pw.clone()];
    let out = r.redact_argv("command", &argv, &mut t);
    assert_eq!(out[0], "agent");
    assert_eq!(out[1], "--password");
    assert!(out[2].starts_with("<redacted:credential-flag-value:"));
    assert!(!out[2].contains(&pw));
}

#[test]
fn argv_inline_flag_value_redacted() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let pw = pw_short();
    let argv = vec!["agent".to_string(), format!("--token={pw}")];
    let out = r.redact_argv("command", &argv, &mut t);
    assert!(out[1].starts_with("--token=<redacted:credential-flag-value:"));
    assert!(!out[1].contains(&pw));
}

#[test]
fn argv_zero_is_not_treated_as_flag_value() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    // argv[0] resembling a flag must not consume a value; it is the binary.
    let argv = vec!["--password".to_string(), "/bin/agent".to_string()];
    let out = r.redact_argv("command", &argv, &mut t);
    assert_eq!(out[0], "--password");
    assert_eq!(out[1], "/bin/agent");
}

#[test]
fn shape_only_skips_flag_value() {
    let r = redactor(RedactMode::ShapeOnly);
    let mut t = RedactionTally::default();
    let pw = pw_short();
    let argv = vec!["agent".to_string(), "--password".to_string(), pw.clone()];
    let out = r.redact_argv("command", &argv, &mut t);
    assert_eq!(out[2], pw); // not shape-matchable, and flag-aware is off
    assert!(t.is_empty());
}

#[test]
fn disabled_unsafe_passes_through() {
    let r = redactor(RedactMode::DisabledUnsafe);
    let mut t = RedactionTally::default();
    let token = gh();
    let out = r.redact_value("f", &token, &mut t);
    assert_eq!(out, token);
    assert!(t.is_empty());
}

#[test]
fn allowlist_suppresses_match() {
    let token = gh();
    let allow = vec![Regex::new(&regex::escape(&token)).unwrap()];
    let r = Redactor::new(RedactMode::ShapeAndFlag, b"k", allow);
    let mut t = RedactionTally::default();
    let out = r.redact_value("f", &token, &mut t);
    assert_eq!(out, token);
    assert!(t.is_empty());
}

#[test]
fn high_entropy_digest_not_flagged() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let digest = format!("sha256:{}", "a1b2c3d4".repeat(8));
    let out = r.redact_value("mcp_tools", &digest, &mut t);
    assert_eq!(out, digest);
    assert!(t.is_empty());
}

#[test]
fn aws_and_credential_assignment_detected() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let _ = r.redact_value("filesystem_paths", &format!("/etc/{}.conf", aws()), &mut t);
    let assignment = format!("run --opt password={}{}", "hunter2", "supersecret");
    let _ = r.redact_value("process_execs", &assignment, &mut t);
    assert_eq!(t.by_rule.get("aws-access-key-id"), Some(&1));
    assert_eq!(t.by_rule.get("credential-assignment"), Some(&1));
}

#[test]
fn sensitive_query_param_catches_assignment_gaps() {
    // access_token / sig / signature are glued or non-keyword, so the credential-assignment rule
    // misses them; the sensitive-query-param rule covers the URL/query case.
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let url = "https://api.example.com/cb?access_token=abcdef123456&sig=deadbeefcafe";
    let out = r.redact_value("network_endpoints", url, &mut t);
    assert!(!out.contains("abcdef123456"));
    assert!(!out.contains("deadbeefcafe"));
    assert_eq!(t.by_rule.get("sensitive-query-param"), Some(&2));
    // host/path are preserved; only the credential params (key=value) are replaced.
    assert!(out.starts_with("https://api.example.com/cb?"));
    assert!(out.contains("<redacted:sensitive-query-param:"));
}

#[test]
fn url_userinfo_redacted_preserving_host() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let pw = format!("s3cr3t{}", "pass");
    let url = format!("https://svcuser:{pw}@db.internal.example.com:5432/app");
    let out = r.redact_url_userinfo("network_endpoints", &url, &mut t);
    assert!(out.starts_with("https://<redacted:url-userinfo:"));
    assert!(out.ends_with("@db.internal.example.com:5432/app"));
    assert!(!out.contains(&pw));
    assert!(!out.contains("svcuser"));
    assert_eq!(t.by_rule.get("url-userinfo"), Some(&1));
}

#[test]
fn url_userinfo_leaves_bare_username_and_hostport() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    // bare username (no password pair) and a plain host:port are not credential pairs
    assert_eq!(
        r.redact_url_userinfo("f", "https://justuser@host.com/x", &mut t),
        "https://justuser@host.com/x"
    );
    assert_eq!(
        r.redact_url_userinfo("f", "10.0.0.1:53", &mut t),
        "10.0.0.1:53"
    );
    assert!(t.is_empty());
}

#[test]
fn url_userinfo_is_idempotent() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let url = "https://u:pw1234@host.com/p";
    let once = r.redact_url_userinfo("f", url, &mut t).into_owned();
    let mut t2 = RedactionTally::default();
    let twice = r.redact_url_userinfo("f", &once, &mut t2);
    assert_eq!(twice, once);
    assert!(t2.is_empty());
}

#[test]
fn find_unredacted_backstop() {
    let r = redactor(RedactMode::ShapeAndFlag);
    assert_eq!(r.find_unredacted("/clean/path"), None);
    assert_eq!(r.find_unredacted(&gh()), Some("github-token"));
    // a placeholder is clean
    let mut t = RedactionTally::default();
    let red = r.redact_value("f", &gh(), &mut t).into_owned();
    assert_eq!(r.find_unredacted(&red), None);
}
