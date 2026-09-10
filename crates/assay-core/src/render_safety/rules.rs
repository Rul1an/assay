//! Curated secret/PII shape rules for render-side redaction (MCP01a).
//!
//! Patterns mirror the capture-side redactor (`assay-runner-core::redact`) and the Plimsoll detector
//! (`plimsoll/secrets.py`) so the three layers share one rule vocabulary. High-signal, low false
//! positive: no generic entropy scan (sha256 digests / content ids are legitimately high-entropy in
//! evidence and must survive). A match is replaced by a value-free `<redacted:RULE>` placeholder; the
//! matched value is never emitted.

use lazy_static::lazy_static;
use regex::Regex;

/// `(rule name, class, compiled pattern)`. `class` is "secret" or "pii" (drives leak accounting).
pub struct Rule {
    pub name: &'static str,
    pub class: &'static str,
    pub re: Regex,
}

lazy_static! {
    pub static ref RULES: Vec<Rule> = vec![
        rule(
            "aws-access-key-id",
            "secret",
            r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"
        ),
        rule("github-token", "secret", r"\bgh[pousr]_[A-Za-z0-9._-]{36,}"),
        rule(
            "github-fine-grained-pat",
            "secret",
            r"\bgithub_pat_[A-Za-z0-9_]{22,}"
        ),
        rule(
            "openai-key",
            "secret",
            r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"
        ),
        rule("slack-token", "secret", r"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"),
        rule("google-api-key", "secret", r"\bAIza[0-9A-Za-z_-]{35}\b"),
        rule(
            "stripe-key",
            "secret",
            r"\b[sp]k_(?:live|test)_[0-9A-Za-z]{16,}\b"
        ),
        rule(
            "private-key-pem",
            "secret",
            r"-----BEGIN (?:[A-Z ]*)PRIVATE KEY-----"
        ),
        rule(
            "jwt",
            "secret",
            r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b",
        ),
        rule(
            "bearer-token",
            "secret",
            r"(?i)\bbearer\s+[A-Za-z0-9._~+/-]{20,}=*"
        ),
        rule(
            "sensitive-query-param",
            "secret",
            r#"(?i)\b(?:access[_-]?token|refresh[_-]?token|sig|signature)=[^&\s#"']{6,}"#,
        ),
        rule(
            "credential-assignment",
            "secret",
            r#"(?i)\b(?:api[_-]?key|secret|token|password|passwd|access[_-]?key|client[_-]?secret)\b\s*[=:]\s*[^\s'"]{6,}"#,
        ),
        rule("email", "pii", r"[\w.+-]+@[\w-]+\.[\w.]+"),
        rule("slack-user-id", "pii", r"\b[UW][A-Z0-9]{8,10}\b"),
    ];
}

fn rule(name: &'static str, class: &'static str, pattern: &str) -> Rule {
    Rule {
        name,
        class,
        re: Regex::new(pattern).expect("render-safety rule pattern is a valid regex"),
    }
}

#[cfg(test)]
mod tests {
    use super::RULES;

    /// This table mirrors the capture-side rules, whose contract is `secret-rules.v1.json`
    /// (ADR-034). Only the runner copy was tested against it, so a change to one copy could leave
    /// the render side behind; the GitHub installation-token format change touched both. Every
    /// shared rule must exist here with a byte-identical pattern; render-only PII rules may be extra.
    #[test]
    fn shared_rules_match_the_secret_rules_contract() {
        // Read at run time, like the attestation page-binding test: a compile-time include of a
        // file outside this crate would break building the tests from the published package.
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../assay-runner-core/tests/fixtures/secret-rules.v1.json");
        let raw = std::fs::read_to_string(&path).expect("the shared secret-rules contract fixture");
        let doc: serde_json::Value = serde_json::from_str(&raw).expect("fixture is valid json");
        assert_eq!(doc["schema"], "assay.secret-rules.v1");
        let shared = doc["rules"].as_array().expect("rules is an array");
        assert!(!shared.is_empty());
        for rule in shared {
            let name = rule["name"].as_str().expect("rule name");
            let pattern = rule["pattern"].as_str().expect("rule pattern");
            let ours = RULES
                .iter()
                .find(|r| r.name == name)
                .unwrap_or_else(|| panic!("render-safety has no rule {name:?} from the contract"));
            assert_eq!(
                ours.re.as_str(),
                pattern,
                "render-safety rule {name:?} drifted from secret-rules.v1.json"
            );
        }
    }
}
