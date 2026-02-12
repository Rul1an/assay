//! Pure helpers: URL parsing, revocation body, digest (no HTTP, no status logic).

use crate::digest::compute_canonical_or_raw_digest;

/// Compute canonical digest of content per SPEC §6.2.
///
/// Uses JCS canonicalization for valid YAML, falls back to raw SHA-256 for
/// non-YAML content (e.g., error responses).
pub(crate) fn compute_digest(content: &str) -> String {
    compute_canonical_or_raw_digest(content, |_| {})
}

/// Parse pack name and version from URL.
///
/// URL format: .../packs/{name}/{version} or .../packs/{name}/{version}.sig
pub(crate) fn parse_pack_url(url: &str) -> (String, String) {
    // Strip .sig suffix if present
    let path = url.split('?').next().unwrap_or(url);
    let path = path.strip_suffix(".sig").unwrap_or(path);

    let parts: Vec<&str> = path.split('/').collect();
    let len = parts.len();

    if len >= 2 {
        (
            parts.get(len - 2).unwrap_or(&"unknown").to_string(),
            parts.get(len - 1).unwrap_or(&"unknown").to_string(),
        )
    } else {
        ("unknown".to_string(), "unknown".to_string())
    }
}

/// Parse 410 revocation response body.
///
/// Expected format: `{"reason": "...", "safe_version": "1.0.1"}`
/// Falls back to header_reason if body parsing fails.
pub(crate) fn parse_revocation_body(
    body: &str,
    header_reason: Option<String>,
) -> (String, Option<String>) {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        let reason = json
            .get("reason")
            .and_then(|v| v.as_str())
            .map(String::from)
            .or(header_reason)
            .unwrap_or_else(|| "no reason provided".to_string());

        let safe_version = json
            .get("safe_version")
            .and_then(|v| v.as_str())
            .map(String::from);

        (reason, safe_version)
    } else {
        let reason = header_reason.unwrap_or_else(|| {
            if body.is_empty() {
                "no reason provided".to_string()
            } else {
                body.chars().take(200).collect()
            }
        });
        (reason, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pack_url() {
        let url = "https://registry.getassay.dev/v1/packs/eu-ai-act/1.2.0";
        let (name, version) = parse_pack_url(url);
        assert_eq!(name, "eu-ai-act");
        assert_eq!(version, "1.2.0");
    }

    #[test]
    fn test_parse_pack_url_with_sig() {
        let url = "https://registry.getassay.dev/v1/packs/eu-ai-act/1.2.0.sig";
        let (name, version) = parse_pack_url(url);
        assert_eq!(name, "eu-ai-act");
        assert_eq!(version, "1.2.0");
    }
}
