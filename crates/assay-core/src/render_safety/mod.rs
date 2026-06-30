//! Render-side sink safety (MCP01a slice 1).
//!
//! A pipeline, not one universal renderer: `strip control -> redact -> truncate -> sink-specific
//! encode`. Control is stripped BEFORE redaction so an attacker cannot hide a secret from the
//! detector by gluing terminal-control bytes into it (`ghp\x1b[m_...` would otherwise break the
//! word boundary, dodge the rule, then surface once control is stripped). The load-bearing invariant
//! is **redact-before-truncate** (so a secret can never survive as a truncated prefix); the final
//! encode is the sink boundary, applied to already-stripped, redacted, bounded text. Capture-side
//! redaction (ADR-034) is a separate, earlier layer; this protects what reaches a rendered sink.
//!
//! Scoped value rule: raw credential values must not appear in public/report sinks. This module does
//! not manage secret lifecycle, rotation or vaulting; detection is pattern-based and may miss a novel
//! format (see `rules`). It is the producer half of the MCP01a render-safety conformance.

pub mod conformance;
pub mod corpus;
pub mod rules;

use lazy_static::lazy_static;
use regex::Regex;
use std::collections::BTreeMap;

/// Default bound for a rendered field, mirroring the Plimsoll sink-safe renderer.
pub const MAX_RENDER_FIELD: usize = 256;
const TRUNCATION_MARKER: &str = "(truncated)";

/// A render sink. The pipeline order is shared; only the final encode differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sink {
    Stdout,
    Json,
    Sarif,
    Junit,
    Markdown,
    Otel,
}

impl Sink {
    pub fn as_str(self) -> &'static str {
        match self {
            Sink::Stdout => "stdout",
            Sink::Json => "json",
            Sink::Sarif => "sarif",
            Sink::Junit => "junit",
            Sink::Markdown => "markdown",
            Sink::Otel => "otel",
        }
    }

    /// The name of the sink-specific final encoding. Structured sinks (json/sarif/otel) return the
    /// value-safe text and let their serializer escape it (`*_serializer` / `attribute_value`); the
    /// adapter must not pre-escape or a downstream serde serializer would double-encode. String-built
    /// sinks (junit/markdown) neutralize active markup in-adapter.
    pub fn encoding(self) -> &'static str {
        match self {
            Sink::Stdout => "terminal_safe",
            Sink::Json | Sink::Sarif => "json_serializer",
            Sink::Junit => "xml_escape",
            Sink::Markdown => "markdown_neutralize",
            Sink::Otel => "attribute_value",
        }
    }

    pub const ALL: [Sink; 6] = [
        Sink::Stdout,
        Sink::Json,
        Sink::Sarif,
        Sink::Junit,
        Sink::Markdown,
        Sink::Otel,
    ];
}

/// The outcome of redaction: the value-free text plus which rule classes fired.
#[derive(Debug, Clone, Default)]
pub struct RedactOutcome {
    pub text: String,
    pub fired: BTreeMap<String, u64>,
    pub secret_hits: u64,
    pub pii_hits: u64,
}

/// Redact secret/PII shapes, replacing each match with a value-free `<redacted:RULE>` placeholder.
/// Idempotent over its own placeholders (they carry no secret shape).
pub fn redact(input: &str) -> RedactOutcome {
    let mut text = input.to_string();
    let mut fired: BTreeMap<String, u64> = BTreeMap::new();
    let mut secret_hits = 0u64;
    let mut pii_hits = 0u64;
    for rule in rules::RULES.iter() {
        let count = rule.re.find_iter(&text).count() as u64;
        if count == 0 {
            continue;
        }
        *fired.entry(rule.name.to_string()).or_insert(0) += count;
        match rule.class {
            "secret" => secret_hits += count,
            "pii" => pii_hits += count,
            _ => {}
        }
        let placeholder = format!("<redacted:{}>", rule.name);
        text = rule
            .re
            .replace_all(&text, placeholder.as_str())
            .into_owned();
    }
    RedactOutcome {
        text,
        fired,
        secret_hits,
        pii_hits,
    }
}

lazy_static! {
    // ESC-introduced sequences: CSI (`ESC [ ... final`) and OSC (`ESC ] ... BEL|ESC\`).
    static ref ANSI_RE: Regex =
        Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)").unwrap();
    // Any remaining lone ESC.
    static ref LONE_ESC: Regex = Regex::new(r"\x1b").unwrap();
    // C0/C1 control (keep \t \n \r), DEL, and Unicode bidi formatting overrides.
    static ref CONTROL_RE: Regex =
        Regex::new(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f\u{80}-\u{9f}\u{202a}-\u{202e}\u{2066}-\u{2069}]").unwrap();
}

/// Strip terminal-control: ANSI/OSC sequences, BEL, other C0/C1 controls (keeping tab/newline/CR),
/// and Unicode bidi overrides. Stripped control becomes U+FFFD so the removal is visible.
pub fn strip_control(input: &str) -> String {
    let no_ansi = ANSI_RE.replace_all(input, "");
    let no_esc = LONE_ESC.replace_all(&no_ansi, "\u{fffd}");
    CONTROL_RE.replace_all(&no_esc, "\u{fffd}").into_owned()
}

/// True if any terminal-control or Unicode bidi-formatting character remains (tab/newline/CR are
/// allowed). The conformance leak predicate for control-class probes: stronger than matching a single
/// corpus needle, it rejects ANY residual control in rendered output.
pub fn has_residual_control(s: &str) -> bool {
    s.chars().any(|c| {
        c == '\u{7f}'
            || ('\u{00}'..='\u{08}').contains(&c)
            || c == '\u{0b}'
            || c == '\u{0c}'
            || ('\u{0e}'..='\u{1f}').contains(&c)
            || ('\u{80}'..='\u{9f}').contains(&c)
            || ('\u{202a}'..='\u{202e}').contains(&c)
            || ('\u{2066}'..='\u{2069}').contains(&c)
    })
}

fn bound(text: &str, max_len: usize) -> String {
    if text.chars().count() <= max_len {
        return text.to_string();
    }
    let truncated: String = text.chars().take(max_len).collect();
    format!("{truncated}{TRUNCATION_MARKER}")
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn markdown_neutralize(s: &str) -> String {
    s.replace('`', "\\`")
        .replace("](", "\\]\\(")
        .replace("![", "\\!\\[")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace("javascript:", "javascript\\:")
}

/// The sink-specific final encode, applied to already-redacted, control-stripped, bounded text.
///
/// Structured sinks (stdout/otel/json/sarif) return the value-safe text unchanged: the value is
/// placed into a JSON/attribute structure whose serializer (serde) applies escaping, so pre-escaping
/// here would double-encode. String-built sinks (junit/markdown) neutralize active markup themselves
/// because their output is often concatenated, not serializer-escaped.
fn encode(sink: Sink, text: &str) -> String {
    match sink {
        Sink::Stdout | Sink::Otel | Sink::Json | Sink::Sarif => text.to_string(),
        Sink::Junit => xml_escape(text),
        Sink::Markdown => markdown_neutralize(text),
    }
}

/// Render `input` safely for `sink`: strip control -> redact -> truncate -> sink-encode.
/// Control-strip precedes redaction (anti-evasion); redact-before-truncate is the leak invariant;
/// encode is the sink boundary on already-safe text. Returns the redaction outcome for accounting.
pub fn render_safe_with_outcome(
    sink: Sink,
    input: &str,
    max_len: usize,
) -> (String, RedactOutcome) {
    let stripped = strip_control(input);
    let redacted = redact(&stripped);
    let bounded = bound(&redacted.text, max_len);
    (encode(sink, &bounded), redacted)
}

/// Render `input` safely for `sink` (see [`render_safe_with_outcome`]).
pub fn render_safe(sink: Sink, input: &str, max_len: usize) -> String {
    render_safe_with_outcome(sink, input, max_len).0
}

/// Object keys whose string values carry untrusted model / agent / tool / user content and must be
/// rendered sink-safe wherever they appear in a `details` tree or result row. Everything not on this
/// list (assay-owned ids, schema names, reason codes, status enums, artifact digests, timestamps,
/// counts, policy ids, fingerprints) stays raw. The list IS the safety boundary: a key matches by
/// name at any depth, and once inside an untrusted-keyed subtree every string leaf is untrusted (so a
/// structured `expected`/`actual`/`assertions[].message` is covered without enumerating its shape).
pub const UNTRUSTED_FIELDS: &[&str] = &[
    "prompt",
    "response",
    "output",
    "error",
    "rationale",
    "message",
    "expected",
    "actual",
    "diff",
    "tool_output",
    "stdout",
    "stderr",
];

fn is_untrusted_key(key: &str) -> bool {
    UNTRUSTED_FIELDS.contains(&key)
}

/// Recursively render the untrusted string leaves of a `details` / result JSON value safe for `sink`,
/// leaving assay-owned keys structurally untouched (byte-stable, not merely no-op'd). A string leaf is
/// untrusted when its own key — or any ancestor key — is in [`UNTRUSTED_FIELDS`]; numbers, bools and
/// null pass through unchanged. This is the recursive allowlist-by-key-name companion to
/// [`render_safe`], for whole-blob sinks (the `run.json` serializer) where each field cannot be wired
/// by hand. `max_len` bounds each rendered leaf; pass [`usize::MAX`] for a record sink that must keep
/// full (redacted) content rather than a truncated preview.
pub fn render_details_safe(
    sink: Sink,
    value: &serde_json::Value,
    max_len: usize,
) -> serde_json::Value {
    render_details_inner(sink, value, max_len, false)
}

fn render_details_inner(
    sink: Sink,
    value: &serde_json::Value,
    max_len: usize,
    in_untrusted: bool,
) -> serde_json::Value {
    use serde_json::Value;
    match value {
        Value::String(s) if in_untrusted => Value::String(render_safe(sink, s, max_len)),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|v| render_details_inner(sink, v, max_len, in_untrusted))
                .collect(),
        ),
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let child_untrusted = in_untrusted || is_untrusted_key(k);
                out.insert(
                    k.clone(),
                    render_details_inner(sink, v, max_len, child_untrusted),
                );
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// DELIBERATELY WRONG order (truncate raw input FIRST, then redact): used only by the differential
/// test to prove that this order leaks a truncated secret prefix. Never call this in product code.
#[doc(hidden)]
pub fn render_truncate_first_unsafe(sink: Sink, input: &str, max_len: usize) -> String {
    let bounded = bound(input, max_len);
    let stripped = strip_control(&bounded);
    let redacted = redact(&stripped);
    encode(sink, &redacted.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_control(s: &str) -> bool {
        s.contains('\u{1b}') || s.contains('\u{07}') || has_residual_control(s)
    }

    #[test]
    fn redacts_secret_shapes_value_free() {
        let token = format!("ghp_{}", "A".repeat(36));
        let out = redact(&format!("here is {token} ok"));
        assert!(out.text.contains("<redacted:github-token>"));
        assert!(!out.text.contains(&token));
        assert_eq!(out.secret_hits, 1);
    }

    #[test]
    fn strips_terminal_control() {
        let s = "\u{1b}[31mRED\u{1b}[0m\u{07}\u{202e}rev";
        let out = strip_control(s);
        assert!(!has_control(&out));
        assert!(out.contains("RED"));
    }

    #[test]
    fn render_safe_never_leaks_across_sinks() {
        let secret = format!("ghp_{}", "B".repeat(36));
        let input = format!("\u{1b}[31m{secret}\u{1b}[0m alice@example.com");
        for sink in Sink::ALL {
            let out = render_safe(sink, &input, MAX_RENDER_FIELD);
            assert!(!out.contains(&secret), "{} leaked secret", sink.as_str());
            assert!(
                !out.contains("alice@example.com"),
                "{} leaked pii",
                sink.as_str()
            );
            assert!(!has_control(&out), "{} leaked control", sink.as_str());
        }
    }

    #[test]
    fn redact_before_truncate_does_not_leak_but_wrong_order_does() {
        // A secret placed near the truncation boundary: truncate-first cuts it so the shape no longer
        // matches and a raw `ghp_` fragment leaks; redact-first replaces it whole before bounding.
        let secret = format!("ghp_{}", "C".repeat(36));
        let input = format!("{} {secret}", "x".repeat(239));
        let safe = render_safe(Sink::Stdout, &input, MAX_RENDER_FIELD);
        assert!(
            !safe.contains("ghp_"),
            "redact-before-truncate must not leak"
        );
        let unsafe_out = render_truncate_first_unsafe(Sink::Stdout, &input, MAX_RENDER_FIELD);
        assert!(
            unsafe_out.contains("ghp_"),
            "truncate-first is expected to leak"
        );
    }

    #[test]
    fn benign_near_matches_survive() {
        let benign =
            "uuid 123e4567-e89b-12d3-a456-426614174000 sha256:deadbeef path /usr/bin/assay";
        let out = redact(benign);
        assert!(
            !out.text.contains("<redacted:"),
            "benign text over-redacted: {}",
            out.text
        );
    }

    #[test]
    fn sink_encodings_are_distinct_where_expected() {
        assert_eq!(Sink::Junit.encoding(), "xml_escape");
        assert_eq!(Sink::Markdown.encoding(), "markdown_neutralize");
        // Structured sinks defer escaping to their serializer (no in-adapter pre-escape).
        assert_eq!(Sink::Sarif.encoding(), "json_serializer");
    }

    #[test]
    fn details_walker_redacts_untrusted_keeps_owned_byte_stable() {
        let secret = format!("ghp_{}", "D".repeat(36));
        let email = "alice@example.com";
        let details = serde_json::json!({
            "prompt": format!("ask {secret}"),
            "assertions": [{ "message": format!("got {email}") }, { "passed": true }],
            "expected": "uuid 123e4567-e89b-12d3-a456-426614174000",
            "skip": { "fingerprint": "abc123def456", "reason": "fingerprint_match" },
            "score_pct": 42,
        });
        let safe = render_details_safe(Sink::Json, &details, usize::MAX);
        let blob = safe.to_string();
        // Untrusted leaves redacted (prompt at any depth, assertions[].message nested).
        assert!(!blob.contains(&secret), "prompt secret leaked");
        assert!(!blob.contains(email), "nested assertion pii leaked");
        assert!(blob.contains("<redacted:"), "no redaction markers fired");
        // Benign content UNDER an untrusted key survives (no over-redaction).
        assert_eq!(
            safe["expected"],
            serde_json::json!("uuid 123e4567-e89b-12d3-a456-426614174000")
        );
        // Assay-owned keys are structurally untouched (not merely a render_safe no-op).
        assert_eq!(
            safe["skip"]["fingerprint"],
            serde_json::json!("abc123def456")
        );
        assert_eq!(
            safe["skip"]["reason"],
            serde_json::json!("fingerprint_match")
        );
        assert_eq!(safe["score_pct"], serde_json::json!(42));
        // A non-string leaf under an untrusted-keyed subtree passes through.
        assert_eq!(safe["assertions"][1]["passed"], serde_json::json!(true));
    }

    #[test]
    fn details_walker_record_sink_keeps_full_length_but_strips_secret() {
        // run.json is a record sink: redact + control-strip, but no truncation marker (usize::MAX).
        let secret = format!("ghp_{}", "E".repeat(36));
        let long = format!("{} {secret}", "z".repeat(400));
        let details = serde_json::json!({ "response": long });
        let safe = render_details_safe(Sink::Json, &details, usize::MAX);
        let rendered = safe["response"].as_str().unwrap();
        assert!(!rendered.contains(&secret), "record sink leaked secret");
        assert!(rendered.contains("<redacted:github-token>"));
        assert!(
            !rendered.contains("(truncated)"),
            "record sink must not truncate"
        );
        assert!(rendered.len() > 400, "record sink preserved full length");
    }

    #[test]
    fn structured_sinks_return_unescaped_value_text() {
        // A benign value with JSON-special chars must come back unescaped, so a downstream serde
        // serializer does not double-encode it. (Active-markup sinks still neutralize in-adapter.)
        let v = r#"path "C:\tmp" <ok>"#;
        assert_eq!(render_safe(Sink::Json, v, MAX_RENDER_FIELD), v);
        assert!(render_safe(Sink::Junit, v, MAX_RENDER_FIELD).contains("&lt;ok&gt;"));
    }
}
