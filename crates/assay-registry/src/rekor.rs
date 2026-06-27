//! MCP04a-3.3c - offline Rekor **v2** inclusion verification.
//!
//! Verifies that a Sigstore bundle's Rekor **v2** transparency-log entry (`hashedrekord` v0.0.2) is
//! included in a **signed checkpoint** under caller-PINNED Rekor verifier material - fully offline.
//!
//! This is a transparency dimension ONLY and is orthogonal to chain / identity / DSSE validity. It does
//! NOT verify the certificate chain, identity, DSSE envelope, subject-digest binding, **timestamp
//! freshness** (Rekor v2 issues no SET; freshness is RFC3161, a separate slice), log **consistency**,
//! **witness** cosignatures, or live log state. A `Verified` here means: this bundle's Rekor v2 entry is
//! included in a checkpoint signed by the pinned log's key - nothing more. `rekor_verified` alone is never
//! a bundle verdict; it only composes with bundle verification (a-3.4).
//!
//! Log attribution is bound, not just "some pinned key verifies": the entry's `logId.keyId` selects the
//! pinned log; the checkpoint signature's key hint must match that log; and the signature must verify under
//! that specific log's key. Exactly one supported v2 entry is allowed. The `canonicalizedBody` is parsed
//! with a STRICT schema (`deny_unknown_fields` + apiVersion/kind/algorithm checks) before its fields are
//! bound to the bundle, so an unsupported body shape cannot leak through as `Verified`.
//!
//! Scoped-policy note (NOT universal spec claims): as Assay's offline v2-conformance policy this verifier
//! additionally requires the checkpoint signature-line name to equal the checkpoint origin, and (when the
//! pinned trusted root carries a `baseUrl`) the checkpoint origin to equal that pinned log host. C2SP only
//! says the key name SHOULD match the origin; the equality and host-binding are Assay's stricter choice.
//!
//! Status precedence (locked): unsupported shape (wrong version / >1 entry / unsupported body) ->
//! `UnsupportedFormat`; missing pinned material -> `TrustRootUnavailable`; missing proof when Required ->
//! `OnlineRequired` (Optional -> `NotPresent`); wrong log / invalid checkpoint signature / unsigned-or-
//! mismatched root / bad inclusion path / leaf-bind mismatch -> `Failed`; valid -> `Verified`.

use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;

use crate::supply_chain::CheckStatus;

mod body;
mod checkpoint;
mod trusted_root;

#[cfg(test)]
mod tests;

use body::HashedRekordBody;
use checkpoint::{b64, parse_checkpoint, rfc6962_root, sha256};
use trusted_root::pinned_tlogs;

pub(super) const HASHEDREKORD_KIND: &str = "hashedrekord";
pub(super) const HASHEDREKORD_V002: &str = "0.0.2";
pub(super) const SUPPORTED_DIGEST_ALG: &str = "SHA2_256";

/// Whether the caller requires transparency-log inclusion. A-3.3c only sets the LOCAL status; the gating
/// decision (does this block?) belongs to the carrier / Plimsoll policy (a-3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransparencyRequirement {
    Required,
    Optional,
}

/// The outcome of offline Rekor v2 inclusion verification: a `CheckStatus` plus a value-free reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RekorInclusionOutcome {
    pub status: CheckStatus,
    pub reason: &'static str,
}

impl RekorInclusionOutcome {
    fn new(status: CheckStatus, reason: &'static str) -> Self {
        Self { status, reason }
    }
}

fn missing_proof(requirement: TransparencyRequirement) -> RekorInclusionOutcome {
    match requirement {
        TransparencyRequirement::Required => RekorInclusionOutcome::new(
            CheckStatus::OnlineRequired,
            "no embedded Rekor inclusion proof and transparency is required",
        ),
        TransparencyRequirement::Optional => RekorInclusionOutcome::new(
            CheckStatus::NotPresent,
            "no embedded Rekor inclusion proof (transparency optional)",
        ),
    }
}

/// Verify a Sigstore bundle's Rekor v2 inclusion proof offline against the pinned trusted-root material.
///
/// `bundle_json` is the Sigstore bundle; `trusted_root_json` is the caller's pinned trust material (only
/// its Ed25519 `tlogs[]` keys are used). See the module docs for the exact (narrow) meaning of `Verified`.
pub fn verify_rekor_v2_inclusion_offline(
    bundle_json: &[u8],
    trusted_root_json: &[u8],
    requirement: TransparencyRequirement,
) -> RekorInclusionOutcome {
    let Ok(bundle) = serde_json::from_slice::<Value>(bundle_json) else {
        return RekorInclusionOutcome::new(CheckStatus::UnsupportedFormat, "malformed bundle");
    };
    let Ok(trusted_root) = serde_json::from_slice::<Value>(trusted_root_json) else {
        return RekorInclusionOutcome::new(
            CheckStatus::TrustRootUnavailable,
            "malformed trusted root",
        );
    };

    // (1) Cardinality + shape (UnsupportedFormat wins first).
    let entries = bundle
        .pointer("/verificationMaterial/tlogEntries")
        .and_then(Value::as_array);
    let entry: Option<&Value> = match entries {
        Some(arr) if arr.len() > 1 => {
            return RekorInclusionOutcome::new(
                CheckStatus::UnsupportedFormat,
                "bundle carries more than one tlog entry",
            )
        }
        Some(arr) if arr.len() == 1 => {
            let e = &arr[0];
            let kind = e.pointer("/kindVersion/kind").and_then(Value::as_str);
            let version = e.pointer("/kindVersion/version").and_then(Value::as_str);
            if kind != Some(HASHEDREKORD_KIND) || version != Some(HASHEDREKORD_V002) {
                return RekorInclusionOutcome::new(
                    CheckStatus::UnsupportedFormat,
                    "not a Rekor v2 hashedrekord 0.0.2 entry",
                );
            }
            Some(e)
        }
        _ => None,
    };

    // (2) Missing pinned material.
    let pinned = pinned_tlogs(&trusted_root);
    if pinned.is_empty() {
        return RekorInclusionOutcome::new(
            CheckStatus::TrustRootUnavailable,
            "no pinned Ed25519 Rekor verifier key in trusted root",
        );
    }

    // (3) Missing proof.
    let Some(entry) = entry else {
        return missing_proof(requirement);
    };
    let Some(ip) = entry.pointer("/inclusionProof").filter(|p| p.is_object()) else {
        return missing_proof(requirement);
    };
    let Some(checkpoint_env) = ip.pointer("/checkpoint/envelope").and_then(Value::as_str) else {
        return missing_proof(requirement);
    };

    // (Log identity) Select the pinned log by the entry's logId; the bundle cannot point at a log we do
    // not pin.
    let Some(entry_log_id) = entry
        .pointer("/logId/keyId")
        .and_then(Value::as_str)
        .and_then(b64)
    else {
        return RekorInclusionOutcome::new(CheckStatus::Failed, "tlog entry has no logId");
    };
    let Some(log) = pinned.iter().find(|p| p.log_id == entry_log_id) else {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "tlog entry references a log not in the pinned trusted root",
        );
    };

    // (4) Checkpoint signature: verify under THIS pinned log's key only, with the key hint and origin
    // bound. The 4-byte hint must equal the selected log id prefix; the signature line name must equal the
    // checkpoint origin; and the Ed25519 signature must verify over the exact signed text.
    let Some(checkpoint) = parse_checkpoint(checkpoint_env) else {
        return RekorInclusionOutcome::new(CheckStatus::Failed, "malformed checkpoint");
    };
    if checkpoint.tree_size == 0 {
        return RekorInclusionOutcome::new(CheckStatus::UnsupportedFormat, "empty checkpoint tree");
    }
    // Bind the checkpoint origin to the OPERATOR-pinned log host (defense-in-depth + explicit
    // attribution). The signature already binds the origin to the key cryptographically; this also
    // requires the verified checkpoint to be for the log the operator pinned. Enforced when the pinned
    // trusted root carries a baseUrl.
    if let Some(expected_origin) = log.origin.as_deref() {
        if checkpoint.origin != expected_origin {
            return RekorInclusionOutcome::new(
                CheckStatus::Failed,
                "checkpoint origin does not match the pinned log",
            );
        }
    }
    let Ok(verifying_key) = VerifyingKey::from_bytes(&log.key) else {
        return RekorInclusionOutcome::new(CheckStatus::TrustRootUnavailable, "invalid pinned key");
    };
    let log_hint = &log.log_id[..log.log_id.len().min(4)];
    let checkpoint_ok = checkpoint.signatures.iter().any(|s| {
        s.name == checkpoint.origin
            && &s.key_hint[..] == log_hint
            && <[u8; 64]>::try_from(s.sig.as_slice())
                .map(|arr| {
                    verifying_key
                        .verify_strict(&checkpoint.signed_text, &Signature::from_bytes(&arr))
                        .is_ok()
                })
                .unwrap_or(false)
    });
    if !checkpoint_ok {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "checkpoint signature does not verify under the pinned log key",
        );
    }

    // (D-ROOT) Only the signed checkpoint root/treeSize is authoritative.
    let ip_root = ip
        .pointer("/rootHash")
        .and_then(Value::as_str)
        .and_then(b64);
    let ip_tree = ip
        .pointer("/treeSize")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<u64>().ok());
    let ip_index = ip
        .pointer("/logIndex")
        .and_then(Value::as_str)
        .and_then(|s| s.parse::<u64>().ok());
    let (Some(ip_root), Some(ip_tree), Some(ip_index)) = (ip_root, ip_tree, ip_index) else {
        return RekorInclusionOutcome::new(CheckStatus::Failed, "malformed inclusion proof fields");
    };
    if ip_root != checkpoint.root_hash || ip_tree != checkpoint.tree_size {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "inclusion proof root/treeSize disagree with the signed checkpoint",
        );
    }

    // (5) Merkle inclusion: leaf = SHA256(0x00 || canonicalizedBody); recompute the root.
    let Some(canonicalized_body) = entry
        .pointer("/canonicalizedBody")
        .and_then(Value::as_str)
        .and_then(b64)
    else {
        return RekorInclusionOutcome::new(CheckStatus::Failed, "missing canonicalizedBody");
    };
    let mut proof_hashes: Vec<[u8; 32]> = Vec::new();
    if let Some(arr) = ip.pointer("/hashes").and_then(Value::as_array) {
        for h in arr {
            let Some(bytes) = h.as_str().and_then(b64) else {
                return RekorInclusionOutcome::new(CheckStatus::Failed, "malformed inclusion hash");
            };
            let Ok(h32): Result<[u8; 32], _> = bytes.try_into() else {
                return RekorInclusionOutcome::new(
                    CheckStatus::UnsupportedFormat,
                    "inclusion proof hash is not 32 bytes",
                );
            };
            proof_hashes.push(h32);
        }
    }
    let leaf_hash = sha256(&[&[0x00], &canonicalized_body]);
    let Some(recomputed) = rfc6962_root(leaf_hash, ip_index, checkpoint.tree_size, &proof_hashes)
    else {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "inclusion proof does not reconstruct the checkpoint root",
        );
    };
    if recomputed[..] != checkpoint.root_hash[..] {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "recomputed inclusion root does not match the signed checkpoint root",
        );
    }

    // (D-LEAF=B) Parse the canonicalizedBody with a STRICT schema, validate the supported shape, and bind
    // its embedded cert + signature (+ artifact digest for messageSignature) to THIS bundle.
    let Ok(body) = serde_json::from_slice::<HashedRekordBody>(&canonicalized_body) else {
        return RekorInclusionOutcome::new(
            CheckStatus::UnsupportedFormat,
            "canonicalizedBody is not a supported hashedrekord v0.0.2 shape",
        );
    };
    if !body.shape_supported() {
        return RekorInclusionOutcome::new(
            CheckStatus::UnsupportedFormat,
            "unsupported canonicalizedBody apiVersion/kind/algorithm",
        );
    }
    let v002 = &body.spec.hashed_rekord_v002;

    let bundle_cert = bundle
        .pointer("/verificationMaterial/certificate/rawBytes")
        .and_then(Value::as_str);
    if bundle_cert != Some(v002.signature.verifier.x509_certificate.raw_bytes.as_str()) {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "logged entry certificate does not match the bundle",
        );
    }
    let bundle_sig = bundle
        .pointer("/messageSignature/signature")
        .or_else(|| bundle.pointer("/dsseEnvelope/signatures/0/sig"))
        .and_then(Value::as_str);
    if bundle_sig != Some(v002.signature.content.as_str()) {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "logged entry signature does not match the bundle",
        );
    }
    if let Some(bundle_digest) = bundle
        .pointer("/messageSignature/messageDigest/digest")
        .and_then(Value::as_str)
    {
        if v002.data.digest != bundle_digest {
            return RekorInclusionOutcome::new(
                CheckStatus::Failed,
                "logged entry artifact digest does not match the bundle",
            );
        }
    }

    RekorInclusionOutcome::new(
        CheckStatus::Verified,
        "Rekor v2 inclusion proof verifies against pinned checkpoint material",
    )
}
