//! Verification policy boundary for Step-2 split.
//!
//! Contract target:
//! - allow/skip/unsigned decisions
//! - fail-closed decision mapping
//! - no crypto/base64/DSSE parsing logic
//! - no IO/network access
//!
//! Forbidden imports (enforced via future grep-gates):
//! - base64::, ed25519_*, sha2::, serde_json::from_slice
//! - reqwest, tokio::fs, std::fs, std::net

use crate::error::RegistryResult;
use crate::trust::TrustStore;
use crate::types::FetchResult;

use super::dsse_next;
use super::errors_next;
use super::wire_next;
use super::{VerifyOptions, VerifyResult};

pub(super) fn verify_pack_impl(
    result: &FetchResult,
    trust_store: &TrustStore,
    options: &VerifyOptions,
) -> RegistryResult<VerifyResult> {
    if let Some(claimed_digest) = &result.headers.digest {
        if claimed_digest != &result.computed_digest {
            return Err(errors_next::digest_mismatch(
                claimed_digest.clone(),
                result.computed_digest.clone(),
            ));
        }
    }

    let signature = &result.headers.signature;
    if signature.is_none() {
        if options.allow_unsigned {
            return Ok(VerifyResult {
                signed: false,
                key_id: None,
                digest: result.computed_digest.clone(),
            });
        }
        return Err(errors_next::unsigned_pack());
    }

    if options.skip_signature {
        return Ok(VerifyResult {
            signed: true,
            key_id: result.headers.key_id.clone(),
            digest: result.computed_digest.clone(),
        });
    }

    let canonical_bytes = wire_next::canonicalize_for_dsse_impl(&result.content)?;
    let sig_b64 = signature
        .as_ref()
        .expect("signature presence already checked in policy");
    let envelope = wire_next::parse_dsse_envelope_impl(sig_b64)?;
    dsse_next::verify_dsse_signature_bytes_impl(&canonical_bytes, &envelope, trust_store)?;

    Ok(VerifyResult {
        signed: true,
        key_id: envelope.signatures.first().map(|s| s.key_id.clone()),
        digest: result.computed_digest.clone(),
    })
}
