use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use sha2::{Digest, Sha256};

pub(super) fn b64(s: &str) -> Option<Vec<u8>> {
    BASE64.decode(s.as_bytes()).ok()
}

pub(super) fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize().into()
}

/// One checkpoint signature line: the key name, the 4-byte key hint, and the raw signature bytes.
pub(super) struct CheckpointSig {
    pub(super) name: String,
    pub(super) key_hint: [u8; 4],
    pub(super) sig: Vec<u8>,
}

/// A parsed C2SP signed-note checkpoint.
pub(super) struct Checkpoint {
    pub(super) signed_text: Vec<u8>,
    pub(super) origin: String,
    pub(super) tree_size: u64,
    pub(super) root_hash: Vec<u8>,
    pub(super) signatures: Vec<CheckpointSig>,
}

/// Parse a checkpoint envelope (C2SP signed note). Body = `origin\n treeSize\n base64(rootHash)\n`
/// (+ optional extension lines), a blank line, then `- <name> base64(keyid[4] || sig)` line(s). The full
/// signed text (everything up to and including the newline before the blank line, extensions included) is
/// preserved for signature verification.
pub(super) fn parse_checkpoint(envelope: &str) -> Option<Checkpoint> {
    let sep = envelope.find("\n\n")?;
    let signed_text = envelope.as_bytes()[..=sep].to_vec();
    let body = &envelope[..sep];
    let sig_block = &envelope[sep + 2..];

    let mut lines = body.split('\n');
    let origin = lines.next()?.to_string();
    let tree_size: u64 = lines.next()?.trim().parse().ok()?;
    let root_hash = b64(lines.next()?.trim())?;
    if root_hash.len() != 32 {
        return None;
    }

    let mut signatures = Vec::new();
    for line in sig_block.split('\n') {
        let line = line.trim_end_matches('\r');
        let Some(rest) = line.strip_prefix("\u{2014} ") else {
            continue;
        };
        let Some((name, b64sig)) = rest.split_once(' ') else {
            continue;
        };
        let Some(decoded) = b64(b64sig) else {
            continue;
        };
        if decoded.len() < 4 {
            continue;
        }
        let mut key_hint = [0u8; 4];
        key_hint.copy_from_slice(&decoded[..4]);
        signatures.push(CheckpointSig {
            name: name.to_string(),
            key_hint,
            sig: decoded[4..].to_vec(),
        });
    }
    Some(Checkpoint {
        signed_text,
        origin,
        tree_size,
        root_hash,
        signatures,
    })
}

/// RFC 6962 section 2.1.1 inclusion-proof verification. Recomputes the tree root from the leaf hash, the
/// leaf index `m`, the tree size `n`, and the proof `hashes` (leaf->root order).
pub(super) fn rfc6962_root(
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
            return None;
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
        return None;
    }
    Some(r)
}
