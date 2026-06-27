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

    /// Redact the userinfo credential pair in a URL (`scheme://user:pass@host` ->
    /// `scheme://<redacted:url-userinfo:H8>@host`), preserving the scheme and host. Only fires when
    /// the userinfo contains a `:` (a `user:pass` pair): a token-as-username is already caught by the
    /// shape pass, while a bare username is not a credential. This is a runner-side capture-hygiene
    /// transform, not a shared detection rule (the password is not shape-matchable), so it is not in
    /// `secret-rules.v1.json`. Idempotent: an already-redacted placeholder is left untouched.
    pub fn redact_url_userinfo<'a>(
        &self,
        field: &str,
        input: &'a str,
        tally: &mut RedactionTally,
    ) -> Cow<'a, str> {
        if !self.mode.is_active() {
            return Cow::Borrowed(input);
        }
        url_userinfo_re().replace_all(input, |caps: &regex::Captures<'_>| {
            let scheme = &caps["scheme"];
            let userinfo = &caps["userinfo"];
            if userinfo.starts_with("<redacted:") || self.is_allowlisted(userinfo) {
                return caps[0].to_string();
            }
            tally.record(field, "url-userinfo");
            format!("{scheme}{}@", self.placeholder("url-userinfo", userinfo))
        })
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

/// Matches `scheme://userinfo@` where the userinfo contains a `user:pass` pair. Captures the scheme
/// and the userinfo so the replacement can keep the scheme and host while redacting only the
/// credential pair.
fn url_userinfo_re() -> &'static Regex {
    use std::sync::OnceLock;
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?P<scheme>[a-zA-Z][a-zA-Z0-9+.-]*://)(?P<userinfo>[^/?#@\s]*:[^/?#@\s]*)@")
            .expect("url userinfo pattern must compile")
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
mod tests;
