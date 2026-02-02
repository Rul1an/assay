use super::config::{AuthConfig, AuthMode};
use super::jwks::JwksProvider;
use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iss: Option<String>,
    pub aud: Option<serde_json::Value>, // Can be string or array
    pub exp: usize,
    pub nbf: Option<usize>,
    pub iat: Option<usize>,
    pub resource: Option<serde_json::Value>, // RFC 8707: string or array
}

pub struct TokenValidator {
    jwks: Option<JwksProvider>,
    static_key: Option<std::sync::Arc<jsonwebtoken::DecodingKey>>,
}

impl TokenValidator {
    pub fn new(jwks: Option<JwksProvider>) -> Self {
        Self {
            jwks,
            static_key: None,
        }
    }

    pub fn new_with_static_key(key_pem: &[u8]) -> anyhow::Result<Self> {
        let key = jsonwebtoken::DecodingKey::from_rsa_pem(key_pem)
            .map_err(|e| anyhow::anyhow!("Failed to create DecodingKey from RSA PEM: {}", e))?;

        Ok(Self {
            jwks: None,
            static_key: Some(std::sync::Arc::new(key)),
        })
    }

    pub async fn validate(&self, token: &str, config: &AuthConfig) -> Result<Claims> {
        // 1. Header Validation (Critical for Alg confusion and Typ discipline)
        // 1. Header Validation (Critical for Alg confusion and Typ discipline)
        // Manual decode to catch 'crit' and others that simple Header struct might miss/drop
        let parts: Vec<&str> = token.split('.').collect();
        if parts.len() != 3 {
            return Err(anyhow::anyhow!("Invalid JWT format"));
        }
        let header_json = URL_SAFE_NO_PAD
            .decode(parts[0])
            .context("Failed to decode JWT header base64")?;
        let header_value: serde_json::Value =
            serde_json::from_slice(&header_json).context("Failed to parse JWT header JSON")?;

        // Hard check for dangerous/unsupported fields
        if let Some(obj) = header_value.as_object() {
            if obj.contains_key("crit") {
                let msg = format!(
                    "Token contains critical extensions ({:?}) which are not understood",
                    obj["crit"]
                );
                if config.mode == AuthMode::Strict {
                    return Err(anyhow::anyhow!(msg));
                }
                tracing::warn!(reason = "W_AUTH_CRIT", "{}", msg);
            }
            if obj.contains_key("jku")
                || obj.contains_key("jwk")
                || obj.contains_key("x5u")
                || obj.contains_key("x5c")
            {
                let msg = "Token contains dangerous headers (jku, jwk, x5u, x5c)";
                if config.mode == AuthMode::Strict {
                    return Err(anyhow::anyhow!(msg));
                }
                tracing::warn!(reason = "W_AUTH_HEADER", "{}", msg);
            }
        }

        let header = decode_header(token).context("Failed to decode JWT header")?;

        // Alg Hardening: Reject 'none' and non-whitelisted algs
        // Alg Hardening: Reject 'none' and non-whitelisted algs
        match header.alg {
            Algorithm::RS256 | Algorithm::ES256 => {} // OK
            _ => {
                let msg = format!("Algorithm {:?} not allowed (only RS256, ES256)", header.alg);
                if config.mode == AuthMode::Strict {
                    return Err(anyhow::anyhow!(msg));
                } else {
                    tracing::warn!(reason = "W_AUTH_ALG", "{}", msg);
                }
            }
        }

        // Typ Discipline (RFC 9068 says 'at+jwt' is recommended)
        if let Some(typ) = &header.typ {
            let t = typ.to_lowercase();
            if t != "jwt" && t != "at+jwt" && t != "application/at+jwt" {
                let msg = format!("Token type '{}' not accepted", typ);
                if config.mode == AuthMode::Strict {
                    return Err(anyhow::anyhow!(msg));
                }
                tracing::warn!(reason = "W_AUTH_TYP", "{}", msg);
            }
        } else if config.mode == AuthMode::Strict {
            // Mandatory in Strict mode per SOTA 2026
            return Err(anyhow::anyhow!("Missing 'typ' header in strict mode"));
        }

        // 2. Key Resolution
        let key = if let Some(sk) = &self.static_key {
            Some(sk.clone())
        } else if let Some(provider) = &self.jwks {
            if let Some(kid) = &header.kid {
                provider.get_key(kid).await.ok()
            } else {
                None
            }
        } else {
            None
        };

        // If no key found and strict -> fail
        let decoding_key = match key {
            Some(k) => k,
            None => {
                if config.mode == AuthMode::Strict && self.jwks.is_some() {
                    return Err(anyhow::anyhow!(
                        "Unable to resolve signing key (kid missing or lookup failed)"
                    ));
                }
                // In permissive, or if no JWKS configured (testing), what do we do?
                // We can use `insecure_disable_signature_validation` FOR TESTING/PERMISSIVE only.
                // But `jsonwebtoken` validation requires a Key.

                // Fallback for Permissive logging: Try decode without verification to show contents/errors
                // This is technically unsafe but allowed in Permissive "audit mode" if explicitly desired.
                // For now, let's treat "Missing Key" as a hard error even in permissive unless we are explicitly "no-auth".
                // But wait, if JWKS is not configured, `jwks` is None.
                if config.jwks_uri.is_none() {
                    // No auth configured.
                    return Err(anyhow::anyhow!("Auth not configured (missing JWKS URI)"));
                }
                return Err(anyhow::anyhow!("Signing key not found"));
            }
        };

        // 3. Validation struct setup
        let mut validation = Validation::new(header.alg);
        validation.leeway = config.clock_skew_leeway.as_secs();

        if let Some(iss) = &config.issuer {
            validation.set_issuer(&[iss]);
        }
        if !config.audience.is_empty() {
            validation.set_audience(&config.audience);
        }

        // 4. Decode & Verify
        let token_data = decode::<Claims>(token, &decoding_key, &validation)
            .map_err(|e| anyhow::anyhow!("JWT validation failed: {}", e))?;

        let claims = token_data.claims;

        // 5. RFC 8707 Resource Validation
        if let Some(expected_resource) = &config.resource_id {
            let authorized = match &claims.resource {
                Some(serde_json::Value::String(s)) => s == expected_resource,
                Some(serde_json::Value::Array(arr)) => {
                    arr.iter().any(|v| v.as_str() == Some(expected_resource))
                }
                _ => false, // Missing or wrong type
            };

            if !authorized {
                let msg = format!(
                    "Resource intent mismatch. Token lacks access to '{}'",
                    expected_resource
                );
                if config.mode == AuthMode::Strict {
                    return Err(anyhow::anyhow!(msg));
                }
                tracing::warn!(reason = "W_AUTH_RESOURCE", "{}", msg);
            }
        }

        Ok(claims)
    }
}
