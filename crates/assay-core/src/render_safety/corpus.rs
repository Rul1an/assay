//! Shared render-safety corpus (MCP01a), mirroring the E38/M3 secret-sink corpus.
//!
//! Two-sided by construction: `HOSTILE` probes MUST be neutralised in every sink; `BENIGN` near
//! matches MUST survive (an over-aggressive renderer must not look "safe" by destroying useful
//! output). Hostile values are obviously-fake shapes; the PEM marker is assembled from split tokens
//! so no contiguous private-key marker is committed.

use lazy_static::lazy_static;
use serde_json::json;
use sha2::{Digest, Sha256};

/// A corpus entry. For HOSTILE, `needle` is the dangerous substring that must be ABSENT from rendered
/// output; for BENIGN, `needle` is an identifying substring that must SURVIVE rendering.
pub struct Probe {
    pub name: &'static str,
    pub class: &'static str, // "secret" | "pii" | "control" | "benign"
    pub input: String,
    pub needle: String,
}

fn p(name: &'static str, class: &'static str, input: String, needle: String) -> Probe {
    Probe {
        name,
        class,
        input,
        needle,
    }
}

lazy_static! {
    /// Assembled from split tokens so no contiguous private-key marker is committed to source.
    static ref FAKE_PEM: String = format!("{}{}{}", "-----BEGIN ", "PRIVATE ", "KEY-----");
    pub static ref HOSTILE: Vec<Probe> = {
        let github = format!("ghp_{}", "A".repeat(36));
        let aws = format!("AKIA{}", "A".repeat(16));
        // Built from split tokens so no contiguous fake-credential literal is committed to source.
        let bearer = format!("Bearer {}", "abcdABCD0123".repeat(2));
        let slack = format!("xoxb-{}", "0123456789abcdef");
        // A word-separated token near the truncation boundary: redact-first replaces it whole;
        // truncate-first cuts it so the shape no longer matches and a raw `ghp_` fragment leaks.
        let boundary = format!("{} ghp_{}", "x".repeat(239), "D".repeat(36));
        vec![
            p("github_pat", "secret", github.clone(), github),
            p("aws_key", "secret", aws.clone(), aws),
            p("bearer_token", "secret", bearer.clone(), bearer),
            p("slack_token", "secret", slack.clone(), slack),
            p("private_key", "secret", FAKE_PEM.clone(), FAKE_PEM.clone()),
            p("email", "pii", "alice@example.com".to_string(), "alice@example.com".to_string()),
            p("slack_user_id", "pii", "U01ABCDEFG".to_string(), "U01ABCDEFG".to_string()),
            p("ansi_escape", "control", "\u{1b}[31mRED\u{1b}[0m".to_string(), "\u{1b}".to_string()),
            p("unicode_bidi", "control", "\u{202e}reversed\u{202c}".to_string(), "\u{202e}".to_string()),
            // Secret near the truncation boundary: guards redact-before-truncate.
            p("long_secret_prefix", "secret", boundary, "ghp_".to_string()),
        ]
    };
    pub static ref BENIGN: Vec<Probe> = vec![
        p("fake_short_token", "benign", "tok_12345".to_string(), "tok_12345".to_string()),
        p(
            "schema_text",
            "benign",
            "assay.mcp_server_inventory.v0".to_string(),
            "mcp_server_inventory".to_string(),
        ),
        p(
            "ordinary_uuid",
            "benign",
            "id 123e4567-e89b-12d3-a456-426614174000".to_string(),
            "123e4567".to_string(),
        ),
        p(
            "content_hash",
            "benign",
            format!("digest sha256:{}", "a".repeat(64)),
            "sha256:".to_string(),
        ),
        p(
            "non_secret_path",
            "benign",
            "/usr/local/bin/assay".to_string(),
            "/usr/local/bin/assay".to_string(),
        ),
    ];
}

/// Deterministic content digest over the corpus (names + inputs), so the conformance report binds the
/// exact corpus it was run against.
pub fn corpus_digest() -> String {
    let value = json!({
        "hostile": HOSTILE.iter().map(|x| json!({"name": x.name, "input": x.input})).collect::<Vec<_>>(),
        "benign": BENIGN.iter().map(|x| json!({"name": x.name, "input": x.input})).collect::<Vec<_>>(),
    });
    let bytes = serde_jcs::to_vec(&value).expect("corpus is JSON-serializable");
    format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
}
