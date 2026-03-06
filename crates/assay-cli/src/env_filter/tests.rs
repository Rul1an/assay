use super::engine::EnvFilter;
use super::matcher::matches_pattern;
use std::collections::HashMap;

fn make_env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn test_pattern_matching() {
    assert!(matches_pattern("AWS_SECRET", "AWS_*"));
    assert!(matches_pattern("GITHUB_TOKEN", "*_TOKEN"));
    assert!(matches_pattern("MY_SECRET_KEY", "*SECRET*"));
    assert!(matches_pattern("PATH", "PATH"));
    assert!(!matches_pattern("PATH", "PATHX"));
    assert!(!matches_pattern("XPATH", "PATH"));
}

#[test]
fn test_scrub_mode_removes_secrets() {
    let env = make_env(&[
        ("PATH", "/usr/bin"),
        ("HOME", "/home/user"),
        ("AWS_SECRET_ACCESS_KEY", "secret"),
        ("GITHUB_TOKEN", "ghp_xxx"),
        ("CUSTOM_VAR", "value"),
    ]);

    let result = EnvFilter::default().filter(&env);

    assert!(result.filtered_env.contains_key("PATH"));
    assert!(result.filtered_env.contains_key("HOME"));
    assert!(result.filtered_env.contains_key("CUSTOM_VAR")); // Unknown passes in Scrub
    assert!(!result.filtered_env.contains_key("AWS_SECRET_ACCESS_KEY"));
    assert!(!result.filtered_env.contains_key("GITHUB_TOKEN"));
    assert_eq!(result.scrubbed_keys.len(), 2);
}

#[test]
fn test_strict_mode_blocks_unknown() {
    let env = make_env(&[
        ("PATH", "/usr/bin"),
        ("HOME", "/home/user"),
        ("CUSTOM_VAR", "value"),
        ("MY_CONFIG", "config"),
    ]);

    let result = EnvFilter::strict().filter(&env);

    assert!(result.filtered_env.contains_key("PATH"));
    assert!(result.filtered_env.contains_key("HOME"));
    assert!(!result.filtered_env.contains_key("CUSTOM_VAR")); // Blocked in Strict
    assert!(!result.filtered_env.contains_key("MY_CONFIG"));
    assert_eq!(result.scrubbed_keys.len(), 2);
}

#[test]
fn test_strict_with_explicit_allow() {
    let env = make_env(&[("PATH", "/usr/bin"), ("CUSTOM_VAR", "value")]);

    let result = EnvFilter::strict()
        .with_allowed(["CUSTOM_VAR"])
        .filter(&env);

    assert!(result.filtered_env.contains_key("PATH"));
    assert!(result.filtered_env.contains_key("CUSTOM_VAR")); // Explicitly allowed
}

#[test]
fn test_exec_influence_stripped() {
    let env = make_env(&[
        ("PATH", "/usr/bin"),
        ("LD_PRELOAD", "/tmp/evil.so"),
        ("PYTHONPATH", "/tmp/evil"),
        ("NODE_OPTIONS", "--require=/tmp/evil.js"),
    ]);

    let result = EnvFilter::default().with_strip_exec(true).filter(&env);

    assert!(result.filtered_env.contains_key("PATH"));
    assert!(!result.filtered_env.contains_key("LD_PRELOAD"));
    assert!(!result.filtered_env.contains_key("PYTHONPATH"));
    assert!(!result.filtered_env.contains_key("NODE_OPTIONS"));
    assert_eq!(result.exec_influence_stripped.len(), 3);
}

#[test]
fn test_exec_influence_allowed_with_warning() {
    let env = make_env(&[("PATH", "/usr/bin"), ("LD_PRELOAD", "/tmp/needed.so")]);

    let result = EnvFilter::default()
        .with_strip_exec(true)
        .with_allowed(["LD_PRELOAD"])
        .filter(&env);

    assert!(result.filtered_env.contains_key("LD_PRELOAD")); // Allowed
    assert_eq!(result.exec_influence_allowed, vec!["LD_PRELOAD"]); // But warned
    assert!(result.exec_influence_stripped.is_empty());
}

#[test]
fn test_passthrough_mode() {
    let env = make_env(&[
        ("PATH", "/usr/bin"),
        ("AWS_SECRET", "secret"),
        ("LD_PRELOAD", "/tmp/lib.so"),
    ]);

    let result = EnvFilter::passthrough().filter(&env);

    assert_eq!(result.filtered_env.len(), 3); // Everything passes
    assert!(result.scrubbed_keys.is_empty());
}

#[test]
fn test_banner_format() {
    let env = make_env(&[("PATH", "/usr/bin"), ("AWS_SECRET", "secret")]);

    let filter = EnvFilter::default();
    let result = filter.filter(&env);
    let banner = filter.format_banner(&result);

    assert!(banner.contains("scrubbed"));
    assert!(banner.contains("1 passed"));
    assert!(banner.contains("1 removed"));
}
