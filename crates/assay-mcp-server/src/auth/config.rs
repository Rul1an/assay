use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum AuthMode {
    /// Log warnings for invalid tokens but allow the request (unless malformed).
    #[default]
    Permissive,
    /// Reject invalid tokens with a hard error.
    Strict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub mode: AuthMode,
    pub jwks_uri: Option<Url>,
    pub issuer: Option<String>,
    pub audience: Vec<String>,
    pub resource_id: Option<String>,
    pub clock_skew_leeway: Duration,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: AuthMode::default(),
            jwks_uri: None,
            issuer: None,
            audience: Vec::new(),
            resource_id: None,
            clock_skew_leeway: Duration::from_secs(30),
        }
    }
}

impl AuthConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();

        if let Ok(v) = env::var("ASSAY_AUTH_MODE") {
            cfg.mode = match v.to_lowercase().as_str() {
                "strict" => AuthMode::Strict,
                _ => AuthMode::Permissive,
            };
        }

        if let Ok(v) = env::var("ASSAY_AUTH_JWKS_URI") {
            if let Ok(u) = Url::parse(&v) {
                if u.scheme() != "https" {
                    if cfg.mode == AuthMode::Strict {
                        eprintln!(
                            "ERROR: JWKS URI must be HTTPS in strict mode. Ignoring unsafe URI."
                        );
                        // In Strict mode, we treat unsafe URI as invalid configuration (None).
                        // This will cause JwksProvider init to fail or skip, preventing startup or auth.
                        cfg.jwks_uri = None;
                    } else {
                        eprintln!("WARN: JWKS URI '{}' is not HTTPS. This is UNSAFE.", v);
                        cfg.jwks_uri = Some(u);
                    }
                } else {
                    cfg.jwks_uri = Some(u);
                }
            }
        }

        if let Ok(v) = env::var("ASSAY_AUTH_ISSUER") {
            cfg.issuer = Some(v);
        }

        if let Ok(v) = env::var("ASSAY_AUTH_AUDIENCE") {
            cfg.audience = v.split(',').map(|s| s.trim().to_string()).collect();
        }

        if let Ok(v) = env::var("ASSAY_AUTH_RESOURCE_ID") {
            cfg.resource_id = Some(v);
        }

        cfg
    }
}
