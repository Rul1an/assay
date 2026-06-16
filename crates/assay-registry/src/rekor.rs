//! MCP04a-3.3c — offline Rekor **v2** inclusion verification.
//!
//! Verifies that a Sigstore bundle's Rekor **v2** transparency-log entry (`hashedrekord` v0.0.2) is
//! included in a **signed checkpoint** under caller-PINNED Rekor verifier material — fully offline.
//!
//! This is a transparency dimension ONLY and is orthogonal to chain / identity / DSSE validity. It does
//! NOT verify the certificate chain, identity, DSSE envelope, subject-digest binding, **timestamp
//! freshness** (Rekor v2 issues no SET; freshness is RFC3161, a separate slice), log **consistency**,
//! **witness** cosignatures, or live log state. A `Verified` here means: this bundle's Rekor v2 entry is
//! included in a checkpoint signed by a pinned Rekor key — nothing more. `rekor_verified` alone is never a
//! bundle verdict; it only composes with bundle verification (a-3.4).
//!
//! Status precedence (locked): unsupported shape -> `UnsupportedFormat`; missing pinned material ->
//! `TrustRootUnavailable`; missing proof when Required -> `OnlineRequired` (Optional -> `NotPresent`);
//! invalid checkpoint signature / unsigned-or-mismatched root / bad inclusion path / leaf-bind mismatch ->
//! `Failed`; valid -> `Verified`.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::supply_chain::CheckStatus;

const HASHEDREKORD_KIND: &str = "hashedrekord";
const HASHEDREKORD_V002: &str = "0.0.2";

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

fn b64(s: &str) -> Option<Vec<u8>> {
    BASE64.decode(s.as_bytes()).ok()
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// Extract pinned Ed25519 verifier keys (raw 32-byte) from a Sigstore trusted-root's `tlogs[]`. The key
/// material is `PKIX_ED25519` SPKI DER (44 bytes); the raw key is the trailing 32 bytes. The ECDSA tlog
/// (old v1 log) is ignored — this slice verifies Ed25519 checkpoints only.
fn pinned_ed25519_keys(trusted_root: &Value) -> Vec<[u8; 32]> {
    let mut keys = Vec::new();
    if let Some(tlogs) = trusted_root.get("tlogs").and_then(Value::as_array) {
        for t in tlogs {
            if t.pointer("/publicKey/keyDetails").and_then(Value::as_str) != Some("PKIX_ED25519") {
                continue;
            }
            let Some(raw) = t
                .pointer("/publicKey/rawBytes")
                .and_then(Value::as_str)
                .and_then(b64)
            else {
                continue;
            };
            let key: Option<[u8; 32]> = match raw.len() {
                44 => raw[12..44].try_into().ok(), // SPKI DER -> trailing 32 raw bytes
                32 => raw[..].try_into().ok(),
                _ => None,
            };
            if let Some(k) = key {
                keys.push(k);
            }
        }
    }
    keys
}

/// A parsed C2SP signed-note checkpoint.
struct Checkpoint {
    /// The exact bytes the signature is computed over: the note text up to AND including the newline
    /// before the blank-line separator.
    signed_text: Vec<u8>,
    tree_size: u64,
    root_hash: Vec<u8>,
    /// Each signature: the raw signature bytes (after the 4-byte C2SP key-id prefix).
    signatures: Vec<Vec<u8>>,
}

/// Parse a checkpoint envelope (C2SP signed note). Body = `origin\n treeSize\n base64(rootHash)\n`
/// (+ optional extension lines), a blank line, then `— <name> base64(keyid[4] || sig)` line(s). Returns
/// `None` if the structure or the three required body lines are malformed.
fn parse_checkpoint(envelope: &str) -> Option<Checkpoint> {
    let sep = envelope.find("\n\n")?;
    let signed_text = envelope.as_bytes()[..=sep].to_vec(); // includes the \n ending the last body line
    let body = &envelope[..sep];
    let sig_block = &envelope[sep + 2..];

    let mut lines = body.split('\n');
    let _origin = lines.next()?;
    let tree_size: u64 = lines.next()?.trim().parse().ok()?;
    let root_hash = b64(lines.next()?.trim())?;
    if root_hash.len() != 32 {
        return None;
    }

    let mut signatures = Vec::new();
    for line in sig_block.split('\n') {
        let line = line.trim_end_matches('\r');
        let Some(rest) = line.strip_prefix("\u{2014} ") else {
            continue; // not a signature line (em-dash + space)
        };
        let Some((_name, b64sig)) = rest.split_once(' ') else {
            continue;
        };
        let Some(decoded) = b64(b64sig) else { continue };
        if decoded.len() < 4 {
            continue;
        }
        signatures.push(decoded[4..].to_vec()); // strip the 4-byte key-id prefix
    }
    Some(Checkpoint {
        signed_text,
        tree_size,
        root_hash,
        signatures,
    })
}

/// RFC 6962 §2.1.1 inclusion-proof verification. Recomputes the tree root from the leaf hash, the leaf
/// index `m`, the tree size `n`, and the proof `hashes` (leaf->root order). Returns the recomputed root,
/// or `None` if indices/proof length are inconsistent.
fn rfc6962_root(
    leaf_hash: [u8; 32],
    mut fnode: u64,
    tree_size: u64,
    hashes: &[[u8; 32]],
) -> Option<[u8; 32]> {
    if fnode >= tree_size {
        return None;
    }
    let mut snode = tree_size - 1;
    let mut r = leaf_hash;
    for p in hashes {
        if snode == 0 {
            return None; // proof too long for this tree size
        }
        if fnode & 1 == 1 || fnode == snode {
            r = sha256(&[&[0x01], p, &r]);
            while fnode & 1 == 0 && fnode != 0 {
                fnode >>= 1;
                snode >>= 1;
            }
        } else {
            r = sha256(&[&[0x01], &r, p]);
        }
        fnode >>= 1;
        snode >>= 1;
    }
    if snode != 0 {
        return None; // proof too short
    }
    Some(r)
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
/// its Ed25519 `tlogs[]` key is used). See the module docs for the exact (narrow) meaning of `Verified`.
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

    let entry = bundle.pointer("/verificationMaterial/tlogEntries/0");

    // (1) Unsupported shape wins first: a present entry that is not Rekor v2 hashedrekord 0.0.2.
    if let Some(entry) = entry.filter(|e| e.is_object()) {
        let kind = entry.pointer("/kindVersion/kind").and_then(Value::as_str);
        let version = entry
            .pointer("/kindVersion/version")
            .and_then(Value::as_str);
        if kind != Some(HASHEDREKORD_KIND) || version != Some(HASHEDREKORD_V002) {
            return RekorInclusionOutcome::new(
                CheckStatus::UnsupportedFormat,
                "not a Rekor v2 hashedrekord 0.0.2 entry",
            );
        }
    }

    // (2) Missing pinned material.
    let keys = pinned_ed25519_keys(&trusted_root);
    if keys.is_empty() {
        return RekorInclusionOutcome::new(
            CheckStatus::TrustRootUnavailable,
            "no pinned Ed25519 Rekor verifier key in trusted root",
        );
    }

    // (3) Missing proof.
    let Some(entry) = entry.filter(|e| e.is_object()) else {
        return missing_proof(requirement);
    };
    let Some(ip) = entry.pointer("/inclusionProof").filter(|p| p.is_object()) else {
        return missing_proof(requirement);
    };
    let Some(checkpoint_env) = ip.pointer("/checkpoint/envelope").and_then(Value::as_str) else {
        return missing_proof(requirement);
    };

    // (4) Checkpoint signature: parse the signed note and require >=1 signature that verifies under a
    // pinned Ed25519 key over the exact signed text. (The pinned key is the anchor; trying each pinned
    // key is sound and avoids depending on a key-id derivation.)
    let Some(checkpoint) = parse_checkpoint(checkpoint_env) else {
        return RekorInclusionOutcome::new(CheckStatus::Failed, "malformed checkpoint");
    };
    let checkpoint_ok = checkpoint.signatures.iter().any(|sig| {
        let Ok(sig_arr): Result<[u8; 64], _> = sig.as_slice().try_into() else {
            return false;
        };
        let signature = Signature::from_bytes(&sig_arr);
        keys.iter().any(|k| {
            VerifyingKey::from_bytes(k)
                .map(|vk| {
                    vk.verify_strict(&checkpoint.signed_text, &signature)
                        .is_ok()
                })
                .unwrap_or(false)
        })
    });
    if !checkpoint_ok {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "checkpoint signature does not verify under any pinned Rekor key",
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

    // (D-LEAF=B) Bind the logged entry to THIS bundle: parse the canonicalizedBody and require its
    // embedded leaf cert + signature (+ artifact digest when present) match the bundle. This proves the
    // logged entry is the bundle's entry, not an unrelated body that happens to be in the log.
    let Ok(body) = serde_json::from_slice::<Value>(&canonicalized_body) else {
        return RekorInclusionOutcome::new(CheckStatus::Failed, "malformed canonicalizedBody JSON");
    };
    let body_cert =
        body.pointer("/spec/hashedRekordV002/signature/verifier/x509Certificate/rawBytes");
    let bundle_cert = bundle.pointer("/verificationMaterial/certificate/rawBytes");
    if body_cert.is_none() || body_cert != bundle_cert {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "logged entry certificate does not match the bundle",
        );
    }
    let body_sig = body.pointer("/spec/hashedRekordV002/signature/content");
    let bundle_sig = bundle
        .pointer("/messageSignature/signature")
        .or_else(|| bundle.pointer("/dsseEnvelope/signatures/0/sig"));
    if body_sig.is_none() || body_sig != bundle_sig {
        return RekorInclusionOutcome::new(
            CheckStatus::Failed,
            "logged entry signature does not match the bundle",
        );
    }
    if let Some(bundle_digest) = bundle.pointer("/messageSignature/messageDigest/digest") {
        let body_digest = body.pointer("/spec/hashedRekordV002/data/digest");
        if body_digest != Some(bundle_digest) {
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
