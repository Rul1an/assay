//! Evidence redaction at capture (ADR-034).
//!
//! Keeps secret-shaped values out of an evidence bundle at the capture boundary, before the archive
//! is hashed and signed, rather than detecting them after they are already persisted. This is the
//! runner-side companion to the consumer-side detector.
//!
//! Design notes (see ADR-034):
//! - Curated provider/structural rules, shared vocabulary with the Plimsoll detector. No generic
//!   entropy rule (digests, UUIDs, and signatures are legitimately high-entropy).
//! - A matched span is replaced by `<redacted:RULE:H8>` where `H8` is the first 8 hex chars of
//!   `HMAC-SHA256(installation_secret, value)`. Deterministic and replay-stable, stable within an
//!   installation (so secret reuse stays visible across runs), and not reversible.
//! - The runner records rule + count only, never the matched value and never its length.
//! - Redaction is a pure transform; it must run before hashing/signing.

use std::borrow::Cow;
use std::collections::BTreeMap;

use regex::Regex;
use sha2::{Digest, Sha256};

/// Redaction mode. Controls how aggressively values are scrubbed at capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactMode {
    /// Default: shape rules plus flag-aware argv value redaction.
    ShapeAndFlag,
    /// Shape rules only (no flag-aware argv value redaction).
    ShapeOnly,
    /// Redaction disabled. Only reachable via `--unsafe-disable-redaction`; the bundle may carry
    /// raw credentials. Recorded in observation_health so a reviewer can see it was not sanitized.
    DisabledUnsafe,
}

impl RedactMode {
    /// The string written into `observation_health.redaction.mode`.
    pub fn as_health_str(self) -> &'static str {
        match self {
            RedactMode::ShapeAndFlag => "shape_and_flag",
            RedactMode::ShapeOnly => "shape_only",
            RedactMode::DisabledUnsafe => "disabled_unsafe",
        }
    }

    fn is_active(self) -> bool {
        !matches!(self, RedactMode::DisabledUnsafe)
    }
}

/// Value-free running totals of what was redacted, for `observation_health.redaction`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RedactionTally {
    pub total: u64,
    pub by_rule: BTreeMap<String, u64>,
    pub by_field: BTreeMap<String, u64>,
}

impl RedactionTally {
    fn record(&mut self, field: &str, rule: &str) {
        self.total += 1;
        *self.by_rule.entry(rule.to_string()).or_insert(0) += 1;
        *self.by_field.entry(field.to_string()).or_insert(0) += 1;
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0
    }
}

/// Argv flag names whose following value is a credential regardless of shape (so a short password,
/// which no shape rule would catch, is still redacted). Case-insensitive compare.
const CRED_FLAGS: &[&str] = &[
    "--token",
    "--api-key",
    "--apikey",
    "--api_key",
    "--key",
    "--secret",
    "--client-secret",
    "--password",
    "--passwd",
    "--pass",
    "-p",
    "--access-key",
    "--access-token",
    "--auth",
    "--authorization",
    "--bearer",
];

/// One curated rule: a name (shared with the Plimsoll detector) and a compiled pattern.
struct Rule {
    name: &'static str,
    re: Regex,
}

/// The capture-boundary redactor. Constructed once per run.
pub struct Redactor {
    mode: RedactMode,
    rules: Vec<Rule>,
    allow: Vec<Regex>,
    salt: Vec<u8>,
}

impl Redactor {
    /// Build a redactor. `salt` is the installation/org secret used to key the placeholder hash;
    /// `allow` are regexes for known-safe values that should never be redacted (false-positive
    /// suppression). The rule patterns are fixed and shared with the Plimsoll detector.
    pub fn new(mode: RedactMode, salt: &[u8], allow: Vec<Regex>) -> Self {
        Self {
            mode,
            rules: build_rules(),
            allow,
            salt: salt.to_vec(),
        }
    }

    pub fn mode(&self) -> RedactMode {
        self.mode
    }

    fn is_allowlisted(&self, value: &str) -> bool {
        self.allow.iter().any(|re| re.is_match(value))
    }

    fn placeholder(&self, rule: &str, matched: &str) -> String {
        let mac = hmac_sha256(&self.salt, matched.as_bytes());
        let h8 = hex::encode(&mac[..4]);
        format!("<redacted:{rule}:{h8}>")
    }

    /// Redact secret-shaped spans in a free-form value (a path, endpoint, tool name, decision).
    /// Returns the (possibly unchanged) value and records any hits in `tally`.
    pub fn redact_value<'a>(
        &self,
        field: &str,
        input: &'a str,
        tally: &mut RedactionTally,
    ) -> Cow<'a, str> {
        if !self.mode.is_active() {
            return Cow::Borrowed(input);
        }
        self.shape_pass(field, input, tally)
    }

    /// Redact an argv vector: flag-aware (a value following a known credential flag is redacted
    /// regardless of shape, in `ShapeAndFlag` mode) plus a shape pass on every token. `argv[0]` is
    /// treated as a binary path (shape pass only, never as a flag value).
    pub fn redact_argv(
        &self,
        field: &str,
        argv: &[String],
        tally: &mut RedactionTally,
    ) -> Vec<String> {
        if !self.mode.is_active() {
            return argv.to_vec();
        }
        let flag_aware = matches!(self.mode, RedactMode::ShapeAndFlag);
        let mut out = Vec::with_capacity(argv.len());
        let mut redact_next_as_value = false;
        for (idx, token) in argv.iter().enumerate() {
            if redact_next_as_value {
                redact_next_as_value = false;
                if !self.is_allowlisted(token) {
                    tally.record(field, "credential-flag-value");
                    out.push(self.placeholder("credential-flag-value", token));
                    continue;
                }
            }

            // `--flag=value` inline form: redact the value half regardless of shape.
            if flag_aware && idx > 0 {
                if let Some((flag, value)) = token.split_once('=') {
                    if is_cred_flag(flag) && !value.is_empty() && !self.is_allowlisted(token) {
                        tally.record(field, "credential-flag-value");
                        out.push(format!(
                            "{flag}={}",
                            self.placeholder("credential-flag-value", value)
                        ));
                        continue;
                    }
                }
                if is_cred_flag(token) {
                    // The credential value is the next token.
                    redact_next_as_value = true;
                    out.push(token.clone());
                    continue;
                }
            }

            out.push(self.shape_pass(field, token, tally).into_owned());
        }
        out
    }

    fn shape_pass<'a>(
        &self,
        field: &str,
        input: &'a str,
        tally: &mut RedactionTally,
    ) -> Cow<'a, str> {
        if self.is_allowlisted(input) {
            return Cow::Borrowed(input);
        }
        // Single left-to-right scan: at each position take the earliest match across all rules
        // (longest on a tie), emit prefix + placeholder, and advance PAST the matched span in the
        // original input. Emitted placeholders are never re-scanned. Existing `<redacted:...>`
        // placeholders already present in the input are copied through untouched, so the pass is
        // idempotent (re-running over redacted text is a no-op) and a placeholder's internal text
        // (e.g. the word "token" in a rule name) cannot trip a later rule.
        let placeholders: Vec<(usize, usize)> = placeholder_re()
            .find_iter(input)
            .map(|m| (m.start(), m.end()))
            .collect();
        let placeholder_end_at = |p: usize| -> Option<usize> {
            placeholders
                .iter()
                .find(|(s, e)| p >= *s && p < *e)
                .map(|(_, e)| *e)
        };

        let mut out = String::new();
        let mut pos = 0usize;
        let mut changed = false;
        while pos <= input.len() {
            let mut best: Option<(usize, usize, &'static str)> = None;
            for rule in &self.rules {
                if let Some(m) = rule.re.find_at(input, pos) {
                    let cand = (m.start(), m.end(), rule.name);
                    best = match best {
                        None => Some(cand),
                        Some(b) if cand.0 < b.0 || (cand.0 == b.0 && cand.1 > b.1) => Some(cand),
                        Some(b) => Some(b),
                    };
                }
            }
            match best {
                Some((start, end, name)) => {
                    // If the match begins inside an existing placeholder, copy through the
                    // placeholder untouched and keep scanning after it.
                    if let Some(ph_end) = placeholder_end_at(start) {
                        out.push_str(&input[pos..ph_end]);
                        pos = ph_end;
                        continue;
                    }
                    let matched = &input[start..end];
                    if self.is_allowlisted(matched) {
                        // Leave this span as is and continue scanning after it.
                        out.push_str(&input[pos..end]);
                        pos = end;
                        continue;
                    }
                    out.push_str(&input[pos..start]);
                    out.push_str(&self.placeholder(name, matched));
                    tally.record(field, name);
                    changed = true;
                    pos = end;
                }
                None => {
                    out.push_str(&input[pos..]);
                    break;
                }
            }
        }
        if changed {
            Cow::Owned(out)
        } else {
            Cow::Borrowed(input)
        }
    }

    /// Backstop for the fail-closed assertion sweep: returns the first rule name that still matches
    /// a non-allowlisted span in `text` (i.e. an unredacted secret slipped through a capture funnel).
    /// `None` means clean. Already-emitted `<redacted:...>` placeholders are stripped before scanning,
    /// so a correctly redacted value is never mistaken for an unredacted one.
    pub fn find_unredacted(&self, text: &str) -> Option<&'static str> {
        let stripped = placeholder_re().replace_all(text, " ");
        for rule in &self.rules {
            for m in rule.re.find_iter(&stripped) {
                if !self.is_allowlisted(m.as_str()) {
                    return Some(rule.name);
                }
            }
        }
        None
    }
}

/// Matches an emitted redaction placeholder `<redacted:RULE:H8>`.
fn placeholder_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"<redacted:[a-z-]+:[0-9a-f]{8}>").expect("placeholder pattern must compile")
    })
}

fn is_cred_flag(flag: &str) -> bool {
    CRED_FLAGS.iter().any(|f| f.eq_ignore_ascii_case(flag))
}

/// Canonical curated rule set: `(name, pattern)`, shared vocabulary with the Plimsoll detector. This
/// is the single source of truth behind the `secret-rules.v1.json` contract fixture; a parity test
/// asserts the two stay in lockstep. Order matters only for which placeholder wins on overlap:
/// provider tokens first, then URL/structural credential shapes, then the broad assignment rule last.
pub fn rule_specs() -> &'static [(&'static str, &'static str)] {
    &[
        ("aws-access-key-id", r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
        ("github-token", r"\bgh[pousr]_[A-Za-z0-9]{36,}\b"),
        ("openai-key", r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"),
        ("slack-token", r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
        ("google-api-key", r"\bAIza[0-9A-Za-z_-]{35}\b"),
        ("stripe-key", r"\b[sp]k_(?:live|test)_[0-9A-Za-z]{16,}\b"),
        ("private-key-pem", r"-----BEGIN (?:[A-Z ]*)PRIVATE KEY-----"),
        (
            "jwt",
            r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
        ),
        ("bearer-token", r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}=*"),
        // URL/query credential params that the assignment rule misses because the credential word is
        // glued to another token by an underscore (e.g. `access_token`) or is not a keyword (`sig`).
        (
            "sensitive-query-param",
            r#"(?i)\b(?:access[_-]?token|refresh[_-]?token|sig|signature)=[^&\s#"']{6,}"#,
        ),
        (
            "credential-assignment",
            r#"(?i)\b(?:api[_-]?key|secret|token|password|passwd|access[_-]?key|client[_-]?secret)\b\s*[=:]\s*[^\s'"]{6,}"#,
        ),
    ]
}

/// The compiled curated rule set.
fn build_rules() -> Vec<Rule> {
    rule_specs()
        .iter()
        .map(|(name, pat)| Rule {
            name,
            re: Regex::new(pat).expect("static redaction pattern must compile"),
        })
        .collect()
}

/// HMAC-SHA256 on top of the already-vendored `sha2` crate (avoids a new dependency for a single
/// keyed hash). Standard construction; used only as a non-reversible redaction salt and key id.
pub(crate) fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        k[..32].copy_from_slice(&digest);
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(msg);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    let out = outer.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

#[cfg(test)]
mod tests {
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
    fn find_unredacted_backstop() {
        let r = redactor(RedactMode::ShapeAndFlag);
        assert_eq!(r.find_unredacted("/clean/path"), None);
        assert_eq!(r.find_unredacted(&gh()), Some("github-token"));
        // a placeholder is clean
        let mut t = RedactionTally::default();
        let red = r.redact_value("f", &gh(), &mut t).into_owned();
        assert_eq!(r.find_unredacted(&red), None);
    }
}
