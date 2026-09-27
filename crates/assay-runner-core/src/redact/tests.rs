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

/// A GitHub App installation token in the stateless format GitHub began rolling out on
/// 2026-04-27: `ghs_<app id>_<JWT>`, about 520 characters with two dots. Assembled from fragments
/// so this file carries no scannable token. The signature ends in `-`, a legal base64url character.
fn stateless_installation_token() -> String {
    let header = format!("ey{}", "JhbGciOiJSUzI1NiIsInR5cCI6IkpXVCJ9");
    let payload = format!("ey{}", "J".to_owned() + &"QmFzZTY0VXJs".repeat(28));
    let signature = format!("{}{}", "c2lnbmF0dXJl_x".repeat(9), "Zz-");
    format!("gh{}_{}_{header}.{payload}.{signature}", "s", "1234567")
}

#[test]
fn shape_pass_redacts_a_stateless_installation_token_whole() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let token = stateless_installation_token();
    let input = format!("/tmp/cfg/{token}");
    let out = r.redact_value("filesystem_paths", &input, &mut t);
    assert!(
        out.contains("<redacted:github-token:"),
        "not attributed: {out}"
    );
    let chars: Vec<char> = token.chars().collect();
    let leaked = chars
        .windows(12)
        .any(|w| out.contains(&w.iter().collect::<String>()));
    assert!(!leaked, "token fragment survived capture redaction: {out}");
    // The signature's final `-` is part of the token; a trailing word boundary would leave it.
    assert!(
        out.ends_with('>'),
        "a trailing token character survived: {out}"
    );
    assert_eq!(t.by_rule.get("github-token"), Some(&1));
    assert_eq!(t.total, 1);
    assert_eq!(r.find_unredacted(&token), Some("github-token"));
}

/// A GitHub fine-grained personal access token: `github_pat_`, 22 characters, `_`, 59 characters.
/// Found by the review of the installation-token change (Muse, claim 3): with no surrounding
/// keyword, header or query context, no rule matched it and it passed capture redaction whole.
fn fine_grained_pat() -> String {
    format!(
        "git{}_pat_{}_{}",
        "hub",
        "11ABCDEFG0123456789abc",
        "Zz9".repeat(19) + "Yy"
    )
}

#[test]
fn shape_pass_redacts_a_contextless_fine_grained_pat() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let mut t = RedactionTally::default();
    let token = fine_grained_pat();
    assert_eq!(token.len(), 93);
    let input = format!("/tmp/{token}.txt");
    let out = r.redact_value("filesystem_paths", &input, &mut t);
    let chars: Vec<char> = token.chars().collect();
    let leaked = chars
        .windows(12)
        .any(|w| out.contains(&w.iter().collect::<String>()));
    assert!(
        !leaked,
        "fine-grained PAT survived capture redaction: {out}"
    );
    assert_eq!(t.by_rule.get("github-fine-grained-pat"), Some(&1));
}

/// Redacts `value` on its own and returns the placeholder the bare secret gets.
fn bare_placeholder(r: &Redactor, secret: &str) -> String {
    let mut t = RedactionTally::default();
    let out = r.redact_value("f", secret, &mut t).into_owned();
    assert_eq!(
        t.total, 1,
        "the bare secret must be exactly one match: {out}"
    );
    out
}

/// A token that names a file keeps the file's extension, and gets the same placeholder it gets
/// anywhere else in the run. Found by a real-runner sweep at v6.1.2: `[A-Za-z0-9._-]{36,}` ran on
/// through `.json`, so the path got a placeholder keyed on `<token>.json`, one secret carried two
/// placeholders, and the reviewer lost the correlation the host-local redaction key exists for.
#[test]
fn a_classic_token_in_a_path_keeps_its_extension_and_its_placeholder() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let token = gh();
    let bare = bare_placeholder(&r, &token);
    for suffix in [".json", ".txt", ".tar.gz", ".backup-2026-09-26"] {
        let mut t = RedactionTally::default();
        let input = format!("/tmp/probe/cfg-0/{token}{suffix}");
        let out = r.redact_value("filesystem_paths", &input, &mut t);
        assert_eq!(out, format!("/tmp/probe/cfg-0/{bare}{suffix}"));
        assert_eq!(t.by_rule.get("github-token"), Some(&1));
    }
}

/// A stateless token whose JWT header is the minimal `{"alg":"RS256"}`: 20 characters, so the
/// app id, `_` and header before the first dot come to fewer than 36. A floor on that part alone
/// would let the whole token through, and the fail-closed sweep, asking the same rule, would pass
/// it. Found by running the real-runner sweep harness against a draft of this change.
fn stateless_installation_token_short_header() -> String {
    let header = format!("ey{}", "JhbGciOiJSUzI1NiJ9");
    let payload = format!("ey{}", "Jpc3MiOiJwcm9iZS1ub3QtYS1zZWNyZXQifQ");
    let signature = format!("{}{}", "UFJPQkUtRkFLRS1TSUdOQVRVUkU", "tbm90LXJlYWw-");
    format!("gh{}_{}_{header}.{payload}.{signature}", "s", "4242424")
}

/// The same holds for the stateless format: the match takes the JWT's two dots and stops at a
/// third, so the signature's trailing `-` is still inside the match and the extension is not.
#[test]
fn a_stateless_token_in_a_path_keeps_its_extension_and_its_placeholder() {
    let r = redactor(RedactMode::ShapeAndFlag);
    for token in [
        stateless_installation_token(),
        stateless_installation_token_short_header(),
    ] {
        let bare = bare_placeholder(&r, &token);
        assert!(
            bare.ends_with('>') && !bare.contains("gh"),
            "bare token survived: {bare}"
        );
        assert_eq!(r.find_unredacted(&token), Some("github-token"));
        // A JWT has exactly two dots, so a third starts something else, however long it is.
        for suffix in [".json", ".backup-2026-09-26"] {
            let mut t = RedactionTally::default();
            let input = format!("/tmp/probe/cfg-0/{token}{suffix}");
            let out = r.redact_value("filesystem_paths", &input, &mut t);
            assert_eq!(out, format!("/tmp/probe/cfg-0/{bare}{suffix}"));
            assert_eq!(t.by_rule.get("github-token"), Some(&1));
        }
    }
}

/// Stopping at an extension must not stop inside a JWT: a stateless token cut off after its
/// header, or part-way through its payload, is still redacted to the last character.
#[test]
fn a_truncated_stateless_token_is_still_redacted_whole() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let token = stateless_installation_token();
    let first_dot = token.find('.').unwrap();
    for cut in [first_dot, first_dot + 40] {
        let fragment = &token[..cut];
        let mut t = RedactionTally::default();
        let input = format!("/tmp/{fragment}");
        let out = r.redact_value("filesystem_paths", &input, &mut t);
        assert!(
            out.ends_with('>') && !out.contains("gh"),
            "truncated token survived: {out}"
        );
        assert_eq!(t.by_rule.get("github-token"), Some(&1));
    }
}

/// The stateless branch takes a JWT only where one starts (`eyJ`), so a file or directory named
/// with a `ghs_` word is not a token.
#[test]
fn a_ghs_word_in_a_path_is_not_a_token() {
    let r = redactor(RedactMode::ShapeAndFlag);
    for path in benign_ghs_paths() {
        let mut t = RedactionTally::default();
        let out = r.redact_value("filesystem_paths", &path, &mut t);
        assert_eq!(out, path);
        assert!(t.is_empty(), "{path}");
        assert_eq!(r.find_unredacted(&path), None, "{path}");
    }
}

/// `ghs_` words in paths, including dotted names whose segments are shorter than a JWT part.
/// Mirrors `benign_ghs_paths` in `assay-core`'s render-safety tests.
fn benign_ghs_paths() -> Vec<String> {
    vec![
        format!("/srv/docs/gh{}_release_notes/v2.md", "s"),
        format!("/srv/docs/gh{}_release_notes.v2.md", "s"),
        format!("/srv/pkg/gh{}_build_artifacts-linux.x86_64-gnu.tar.gz", "s"),
    ]
}

/// The partial-token branch recognises a stateless token from the `eyJ` that begins its JWT header
/// onward. A cut at or after that boundary is redacted whole; a cut before it (the prefix and app id
/// alone, or with only `e` or `ey`) holds no JWT bytes, is not claimed, and stays as it is.
#[test]
fn a_cut_stateless_token_is_recognised_from_the_eyj_header_boundary() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let app = format!("gh{}_4242424_", "s");
    for cut in ["eyJ", "eyJh", "eyJhbGciOiJSUzI1NiJ9"] {
        let fragment = format!("{app}{cut}");
        let mut t = RedactionTally::default();
        let out = r.redact_value("f", &fragment, &mut t);
        assert!(
            out.starts_with("<redacted:github-token:") && out.ends_with('>'),
            "{fragment}: {out}"
        );
        assert_eq!(r.find_unredacted(&fragment), Some("github-token"));
    }
    for cut in ["", "e", "ey"] {
        let fragment = format!("{app}{cut}");
        let mut t = RedactionTally::default();
        assert_eq!(r.redact_value("f", &fragment, &mut t), fragment);
        assert!(t.is_empty(), "{fragment}");
    }
}

/// A token cut exactly after `header.payload` and followed directly by `.json` is redacted with the
/// `.json`. Without path context, a short signature fragment and a file extension have the same
/// shape; the rule takes it as the signature so the secret fails closed, at the cost of the name.
/// This is a stated non-claim, not a distinction the rule makes.
#[test]
fn a_cut_after_the_payload_takes_a_following_extension_as_the_signature() {
    let r = redactor(RedactMode::ShapeAndFlag);
    let fragment = format!(
        "gh{}_4242424_ey{}.ey{}",
        "s", "JhbGciOiJSUzI1NiJ9", "Jpc3MiOiJwcm9iZS1ub3QtYS1zZWNyZXQifQ"
    );
    let bare = bare_placeholder(&r, &fragment);
    let with_extension = bare_placeholder(&r, &format!("{fragment}.json"));
    // One placeholder covers fragment and extension: nothing of `.json` is left outside it.
    assert!(
        with_extension.starts_with("<redacted:github-token:") && with_extension.ends_with('>'),
        "the extension is not part of the match: {with_extension}"
    );
    assert_ne!(with_extension, bare, "the extension is part of the match");
    let mut t = RedactionTally::default();
    let input = format!("/tmp/probe/{fragment}.json");
    let out = r.redact_value("filesystem_paths", &input, &mut t);
    assert_eq!(out, format!("/tmp/probe/{with_extension}"));
    assert_eq!(t.by_rule.get("github-token"), Some(&1));
}
