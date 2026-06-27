use serde::Deserialize;

use super::{HASHEDREKORD_KIND, HASHEDREKORD_V002, SUPPORTED_DIGEST_ALG};

// --- strict HashedRekord v0.0.2 body schema (deny_unknown_fields rejects unsupported shapes) ---

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HashedRekordBody {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    pub(super) spec: BodySpec,
}

impl HashedRekordBody {
    /// Whether this is the single supported entry shape: hashedrekord v0.0.2 with a SHA2_256 digest.
    pub(super) fn shape_supported(&self) -> bool {
        self.api_version == HASHEDREKORD_V002
            && self.kind == HASHEDREKORD_KIND
            && self.spec.hashed_rekord_v002.data.algorithm == SUPPORTED_DIGEST_ALG
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodySpec {
    #[serde(rename = "hashedRekordV002")]
    pub(super) hashed_rekord_v002: BodyV002,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyV002 {
    pub(super) data: BodyData,
    pub(super) signature: BodySignature,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyData {
    pub(super) algorithm: String,
    pub(super) digest: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodySignature {
    pub(super) content: String,
    pub(super) verifier: BodyVerifier,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyVerifier {
    #[serde(rename = "keyDetails")]
    #[allow(dead_code)]
    key_details: String,
    #[serde(rename = "x509Certificate")]
    pub(super) x509_certificate: BodyCert,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BodyCert {
    #[serde(rename = "rawBytes")]
    pub(super) raw_bytes: String,
}
