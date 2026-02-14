//! Verify orchestration/policy boundary for split.
//!
//! Contract target:
//! - verification flow orchestration and policy decisions only
//! - no low-level DSSE crypto primitives
//! - no base64/wire parsing internals

use crate::error::{RegistryError, RegistryResult};
use crate::trust::TrustStore;
use crate::types::FetchResult;

use super::super::{VerifyOptions, VerifyResult};
use super::dsse::{canonicalize_for_dsse_impl, verify_dsse_signature_bytes_impl};
use super::wire::parse_dsse_envelope_impl;

pub(crate) fn verify_pack_impl(
    result: &FetchResult,
    trust_store: &TrustStore,
    options: &VerifyOptions,
) -> RegistryResult<VerifyResult> {
    if let Some(claimed_digest) = &result.headers.digest {
        if claimed_digest != &result.computed_digest {
            return Err(RegistryError::DigestMismatch {
                name: "pack".to_string(),
                version: "unknown".to_string(),
                expected: claimed_digest.clone(),
                actual: result.computed_digest.clone(),
            });
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
        return Err(RegistryError::Unsigned {
            name: "pack".to_string(),
            version: "unknown".to_string(),
        });
    }

    if options.skip_signature {
        return Ok(VerifyResult {
            signed: true,
            key_id: result.headers.key_id.clone(),
            digest: result.computed_digest.clone(),
        });
    }

    let canonical_bytes = canonicalize_for_dsse_impl(&result.content)?;
    let sig_b64 = signature
        .as_ref()
        .expect("signature presence already checked in policy");
    let envelope = parse_dsse_envelope_impl(sig_b64)?;
    verify_dsse_signature_bytes_impl(&canonical_bytes, &envelope, trust_store)?;

    Ok(VerifyResult {
        signed: true,
        key_id: envelope.signatures.first().map(|s| s.key_id.clone()),
        digest: result.computed_digest.clone(),
    })
}
