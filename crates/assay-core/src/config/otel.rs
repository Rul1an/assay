use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default)]
pub struct OtelConfig {
    /// GenAI Semantic Conventions version to anchor span attributes.
    /// Default: "1.28.0" (Bleeding Edge 2026)
    pub genai_semconv_version: String,

    /// Stability control for attributes.
    /// "stable_only" (default) or "experimental_opt_in".
    pub semconv_stability: SemConvStability,

    /// Privacy control for prompt/response payloads.
    /// "off" (default and invariant) MUST NOT emit payloads inline.
    #[serde(rename = "capture_mode")]
    pub capture_mode: PromptCaptureMode,

    /// Redaction Settings (if capture is enabled).
    pub redaction: RedactionConfig,

    /// Telemetry Surface Guardrails (Anti-OpenClaw).
    pub exporter: ExporterConfig,

    /// explicit acknowledgement required to enable capture (Two-person rule/Anti-misconfig).
    #[serde(default)]
    pub capture_acknowledged: bool,

    /// Whether to require a sampled span before capturing payloads (prevents ghost costs).
    #[serde(default = "default_true")]
    pub capture_requires_sampled_span: bool,
}

fn default_true() -> bool {
    true
}

impl Default for OtelConfig {
    fn default() -> Self {
        Self {
            genai_semconv_version: "1.28.0".to_string(),
            semconv_stability: SemConvStability::default(),
            capture_mode: PromptCaptureMode::default(),
            redaction: RedactionConfig::default(),
            exporter: ExporterConfig::default(),
            capture_acknowledged: false,
            capture_requires_sampled_span: true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum SemConvStability {
    #[default]
    StableOnly,
    ExperimentalOptIn,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PromptCaptureMode {
    #[default]
    Off,
    RedactedInline,
    BlobRef,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct RedactionConfig {
    // Basic regex-based redactions
    #[serde(default)]
    pub policies: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Default)]
pub struct ExporterConfig {
    // Explicit allowlist for OTLP destinations if payload capture is on
    #[serde(default)]
    pub allowlist: Option<Vec<String>>,

    /// Allow binding/exporting to localhost loopback (Anti-OpenClaw debug surface protection).
    /// Default: false (Deny)
    #[serde(default)]
    pub allow_localhost: bool,
}

impl OtelConfig {
    pub fn validate(&self) -> Result<(), String> {
        if matches!(self.capture_mode, PromptCaptureMode::Off) {
            return Ok(());
        }

        // 0. Anti-Misconfiguration Guard (Two-person rule)
        if !self.capture_acknowledged {
            return Err(
                "OpenClaw: 'otel.capture_acknowledged' must be true when capture_mode is enabled."
                    .to_string(),
            );
        }

        // OpenClaw Guardrails: If capture is enabled, strict security is required.

        // 1. TLS Enforcement
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").unwrap_or_default();
        if !endpoint.is_empty()
            && !endpoint.starts_with("https://")
            && !endpoint.starts_with("http://localhost")
        {
            // Allow localhost for dev, but require HTTPS for remote
            return Err(
                "OpenClaw: OTLP endpoint must use TLS (https://) when payload capture is enabled."
                    .to_string(),
            );
        }

        // 2. Explicit Allowlist
        if let Some(list) = &self.exporter.allowlist {
            if !endpoint.is_empty() {
                // Check if endpoint domain/prefix is in allowlist (Wildcard Support)
                let allowed = list
                    .iter()
                    .any(|rule| Self::matches_allowlist(&endpoint, rule));
                if !allowed {
                    return Err(format!(
                        "OpenClaw: OTLP endpoint '{}' is not in the explicit allowlist.",
                        endpoint
                    ));
                }
            }
        } else {
            // If capture is ON, allowlist is MANDATORY
            return Err("OpenClaw: An explicit 'exporter.allowlist' is required when payload capture is enabled.".to_string());
        }

        // 3. Localhost Binding Guard
        if !self.exporter.allow_localhost
            && (endpoint.contains("localhost")
                || endpoint.contains("127.0.0.1")
                || endpoint.contains("::1"))
        {
            return Err("OpenClaw: Export to localhost is blocked by default. Set 'exporter.allow_localhost = true' to enable.".to_string());
        }

        // 4. BlobRef: ASSAY_ORG_SECRET required (no ephemeral key in prod; hashes would be guessable across installs).
        if matches!(self.capture_mode, PromptCaptureMode::BlobRef) {
            let secret = std::env::var("ASSAY_ORG_SECRET").unwrap_or_default();
            if secret.is_empty() || secret == "ephemeral-key" {
                return Err("OpenClaw: BlobRef mode requires ASSAY_ORG_SECRET to be set (no ephemeral key).".to_string());
            }
        }

        Ok(())
    }

    /// Check if host matches allowlist rule (Exact or *.wildcard).
    /// Uses strict URL parsing to avoid substring/ipv6 bypasses.
    fn matches_allowlist(endpoint: &str, rule: &str) -> bool {
        // Use parsing to extract host reliably
        let host_str = if endpoint.contains("://") {
            if let Ok(url) = url::Url::parse(endpoint) {
                url.host_str().map(|h| h.to_string())
            } else {
                None // Invalid URL, block it safely
            }
        } else {
            // Fallback: split by colon if no scheme.
            endpoint.split(':').next().map(|s| s.to_string())
        };

        let Some(host) = host_str else {
            // Fail closed if we can't parse host
            return false;
        };

        // Host normalization (lowercase)
        let host = host.to_lowercase();
        let rule = rule.to_lowercase();

        if rule.starts_with("*.") {
            let suffix = &rule[1..]; // keep dot: ".trusted.org"
            host.ends_with(suffix) && !host.strip_suffix(suffix).unwrap_or("").contains('.')
        } else {
            host == rule
        }
    }
}

#[cfg(test)]
#[allow(unsafe_code)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    #[serial]
    fn test_guardrails_validation() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::RedactedInline;
        cfg.capture_acknowledged = true;
        cfg.exporter.allowlist = None; // Ensure reset

        // 1. Unset env var -> Error (No allowlist provided)
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        let res = cfg.validate();
        assert!(
            res.is_err(),
            "Should fail without allowlist when capture is on"
        );

        // 2. Set Allowlist, but bad Endpoint (HTTP)
        cfg.exporter.allowlist = Some(vec!["example.com".to_string()]);

        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "http://example.com");
        }
        let res = cfg.validate();
        assert!(res.is_err(), "Should fail HTTP endpoint");

        // 3. Good Endpoint (HTTPS + Allowlist match)
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://example.com");
        }
        let res = cfg.validate();
        assert!(res.is_ok(), "Should pass HTTPS + Allowlist");

        // 4. Boundary/Attack Tests (Audit Requirement)
        cfg.exporter.allowlist = Some(vec!["example.com".to_string(), "*.trusted.org".to_string()]);

        // Case A: Suffix Attack (example.com.attacker.tld)
        unsafe {
            std::env::set_var(
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "https://example.com.attacker.tld",
            );
        }
        assert!(cfg.validate().is_err(), "Must block suffix spoofing");

        // Case B: Prefix Attack (evilexample.com)
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://evilexample.com");
        }
        assert!(cfg.validate().is_err(), "Must block prefix spoofing");

        // Case C: Trusted Wildcard
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://api.trusted.org");
        }
        assert!(cfg.validate().is_ok(), "Must allow valid wildcard child");

        // 5. Clean up
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
    }

    /// Sign-off: allowlist wildcard *.mycorp.com allows otel.mycorp.com; denies evilmycorp.com (no substring).
    #[test]
    #[serial]
    fn test_allowlist_wildcard_mycorp_allowed_evil_denied() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::BlobRef;
        cfg.capture_acknowledged = true;
        cfg.exporter.allowlist = Some(vec!["*.mycorp.com".to_string()]);

        unsafe {
            std::env::set_var("ASSAY_ORG_SECRET", "test-secret");
        }
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otel.mycorp.com");
        }
        assert!(
            cfg.validate().is_ok(),
            "*.mycorp.com must allow https://otel.mycorp.com"
        );

        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://evilmycorp.com");
        }
        assert!(
            cfg.validate().is_err(),
            "*.mycorp.com must NOT allow https://evilmycorp.com (substring bypass)"
        );

        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        unsafe {
            std::env::remove_var("ASSAY_ORG_SECRET");
        }
    }

    /// Sign-off: port and trailing-dot edge cases (host extraction via url crate).
    #[test]
    #[serial]
    fn test_allowlist_port_and_trailing_dot() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::RedactedInline;
        cfg.capture_acknowledged = true;
        cfg.exporter.allowlist = Some(vec!["otel.mycorp.com".to_string()]);

        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://otel.mycorp.com:443");
        }
        assert!(
            cfg.validate().is_ok(),
            "Host with port must match by host only"
        );

        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
    }

    /// Sign-off: allow_localhost default deny; explicit true allows localhost.
    #[test]
    #[serial]
    fn test_allow_localhost_default_deny_explicit_true_allowed() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::BlobRef;
        cfg.capture_acknowledged = true;
        cfg.exporter.allowlist = Some(vec!["127.0.0.1".to_string()]);
        cfg.exporter.allow_localhost = false;

        unsafe {
            std::env::set_var("ASSAY_ORG_SECRET", "test-secret");
        }
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://127.0.0.1");
        }
        assert!(
            cfg.validate().is_err(),
            "allow_localhost=false must block localhost"
        );

        cfg.exporter.allow_localhost = true;
        assert!(
            cfg.validate().is_ok(),
            "allow_localhost=true must allow when in allowlist"
        );

        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
        unsafe {
            std::env::remove_var("ASSAY_ORG_SECRET");
        }
    }

    /// Sign-off: BlobRef requires ASSAY_ORG_SECRET (fail when unset or ephemeral-key).
    #[test]
    #[serial]
    fn test_blob_ref_requires_assay_org_secret() {
        let mut cfg = OtelConfig::default();
        cfg.capture_mode = PromptCaptureMode::BlobRef;
        cfg.capture_acknowledged = true;
        cfg.exporter.allowlist = Some(vec!["example.com".to_string()]);

        unsafe {
            std::env::remove_var("ASSAY_ORG_SECRET");
        }
        unsafe {
            std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", "https://example.com");
        }
        assert!(
            cfg.validate().is_err(),
            "BlobRef must fail when ASSAY_ORG_SECRET unset"
        );

        unsafe {
            std::env::set_var("ASSAY_ORG_SECRET", "ephemeral-key");
        }
        assert!(
            cfg.validate().is_err(),
            "BlobRef must fail when ASSAY_ORG_SECRET is ephemeral-key"
        );

        unsafe {
            std::env::set_var("ASSAY_ORG_SECRET", "prod-secret-xyz");
        }
        assert!(
            cfg.validate().is_ok(),
            "BlobRef must pass when ASSAY_ORG_SECRET set"
        );

        unsafe {
            std::env::remove_var("ASSAY_ORG_SECRET");
        }
        unsafe {
            std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        }
    }
}
