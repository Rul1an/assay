use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[deprecated(note = "stdio authentication is unsupported; this legacy type is compatibility-only")]
pub struct Claims {
    pub sub: String,
    pub iss: Option<String>,
    pub aud: Option<serde_json::Value>, // Can be string or array
    pub exp: usize,
    pub nbf: Option<usize>,
    pub iat: Option<usize>,
    pub resource: Option<serde_json::Value>, // RFC 8707: string or array
}
