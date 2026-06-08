//! Redaction key resolution (ADR-034).
//!
//! The redaction placeholder hash is keyed by an installation/org-scoped secret so that the same
//! credential redacts to the same `<redacted:RULE:H8>` token across runs (reuse stays visible), while
//! staying non-reversible and not globally correlatable. The key is a runner-local file, generated
//! once, with an env override for CI/ephemeral runners.
//!
//! Resolution order (see `resolve`):
//! 1. `ASSAY_REDACTION_KEY_FILE` env var, a path to an existing key file.
//! 2. The default host-local key file, generated once (0600) if absent.
//! 3. Ephemeral: an in-memory random key, only when explicitly requested. Never persisted.
//!
//! The key itself is never written into the bundle and never logged. Evidence records only a
//! non-reversible `key_id` and the `key_scope`.

use std::fs;
use std::io;
use std::path::Path;

use uuid::Uuid;

use crate::redact::hmac_sha256;

/// File content prefix / version tag for a redaction key file.
pub const KEY_FILE_PREFIX: &str = "assay-redaction-key-v1:";
/// Env var holding an explicit path to a redaction key file (CI / mounted secret).
pub const ENV_KEY_FILE: &str = "ASSAY_REDACTION_KEY_FILE";

/// Where a redaction key came from, recorded in `observation_health.redaction.key_scope`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyScope {
    /// A persisted host-local key file (default path or env-provided). Tokens correlate across runs.
    HostLocal,
    /// An in-memory throwaway key. Tokens do not correlate with any other run.
    Ephemeral,
}

impl KeyScope {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyScope::HostLocal => "host_local",
            KeyScope::Ephemeral => "ephemeral",
        }
    }
}

/// A resolved redaction key: the salt bytes plus the value-free metadata recorded in evidence.
#[derive(Debug, Clone)]
pub struct RedactionKey {
    bytes: Vec<u8>,
    scope: KeyScope,
    key_id: String,
}

impl RedactionKey {
    fn from_bytes(bytes: Vec<u8>, scope: KeyScope) -> Self {
        let key_id = compute_key_id(&bytes);
        Self {
            bytes,
            scope,
            key_id,
        }
    }

    /// The salt bytes for the redactor. Never logged, never serialized.
    pub fn salt(&self) -> &[u8] {
        &self.bytes
    }

    pub fn scope(&self) -> KeyScope {
        self.scope
    }

    /// Non-reversible key id (`hmac-sha256:<8 hex>`), safe to record in evidence. Lets a reviewer tell
    /// whether two bundles share a redaction domain without ever seeing the key.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    /// An in-memory ephemeral key (explicit `--redaction-key ephemeral`).
    pub fn ephemeral() -> Self {
        Self::from_bytes(generate_key_bytes().to_vec(), KeyScope::Ephemeral)
    }

    /// Resolve a host-local key: read `ASSAY_REDACTION_KEY_FILE` if set, else read the default path,
    /// generating and persisting (0600) a fresh key there if it does not yet exist. `env_override` and
    /// `default_path` are passed in (not read from globals) so this is testable.
    pub fn resolve_host_local(
        env_override: Option<&Path>,
        default_path: &Path,
    ) -> io::Result<Self> {
        if let Some(path) = env_override {
            let bytes = read_key_file(path)?;
            return Ok(Self::from_bytes(bytes, KeyScope::HostLocal));
        }
        if default_path.exists() {
            let bytes = read_key_file(default_path)?;
            return Ok(Self::from_bytes(bytes, KeyScope::HostLocal));
        }
        let bytes = generate_key_bytes().to_vec();
        write_key_file(default_path, &bytes)?;
        Ok(Self::from_bytes(bytes, KeyScope::HostLocal))
    }
}

/// 32 random bytes, sourced from two v4 UUIDs (CSPRNG-backed via the already-vendored `uuid` crate,
/// so no new randomness dependency). Ample entropy for a redaction salt.
pub fn generate_key_bytes() -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..16].copy_from_slice(Uuid::new_v4().as_bytes());
    out[16..].copy_from_slice(Uuid::new_v4().as_bytes());
    out
}

fn compute_key_id(bytes: &[u8]) -> String {
    // Keyed self-digest: HMAC-SHA256(key, fixed label), first 8 hex. Non-reversible.
    let mac = hmac_sha256(bytes, b"assay-redaction-key-id");
    format!("hmac-sha256:{}", hex::encode(&mac[..4]))
}

/// Encode a key file body: `assay-redaction-key-v1:<base64url(bytes)>`.
pub fn encode_key_file(bytes: &[u8]) -> String {
    format!("{KEY_FILE_PREFIX}{}", base64url_encode(bytes))
}

/// Parse a key file body, validating the version prefix.
pub fn parse_key_file(content: &str) -> io::Result<Vec<u8>> {
    let trimmed = content.trim();
    let b64 = trimmed.strip_prefix(KEY_FILE_PREFIX).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "redaction key file missing expected version prefix",
        )
    })?;
    base64url_decode(b64)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "redaction key is not base64url"))
}

fn read_key_file(path: &Path) -> io::Result<Vec<u8>> {
    let content = fs::read_to_string(path)?;
    parse_key_file(&content)
}

fn write_key_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, encode_key_file(bytes))?;
    set_owner_only_perms(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_owner_only_perms(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_owner_only_perms(_path: &Path) -> io::Result<()> {
    Ok(())
}

// --- base64url (URL-safe, no padding). Hand-rolled to avoid a new dependency. ---

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn base64url_encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(B64URL[((n >> 18) & 0x3f) as usize] as char);
        out.push(B64URL[((n >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64URL[((n >> 6) & 0x3f) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[(n & 0x3f) as usize] as char);
        }
    }
    out
}

fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a' + 26) as u32),
            b'0'..=b'9' => Some((c - b'0' + 52) as u32),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    for chunk in bytes.chunks(4) {
        if chunk.len() < 2 {
            return None;
        }
        let mut n = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            n |= val(c)? << (18 - 6 * i);
        }
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_round_trips() {
        for len in 0..40 {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 7 + 3) as u8).collect();
            let enc = base64url_encode(&bytes);
            assert!(!enc.contains('+') && !enc.contains('/') && !enc.contains('='));
            assert_eq!(base64url_decode(&enc).unwrap(), bytes);
        }
    }

    #[test]
    fn key_file_round_trips_with_prefix() {
        let bytes = generate_key_bytes();
        let body = encode_key_file(&bytes);
        assert!(body.starts_with(KEY_FILE_PREFIX));
        assert_eq!(parse_key_file(&body).unwrap(), bytes.to_vec());
    }

    #[test]
    fn parse_rejects_missing_prefix() {
        assert!(parse_key_file("not-a-key").is_err());
    }

    #[test]
    fn key_id_is_stable_and_not_the_key() {
        let bytes = generate_key_bytes();
        let k = RedactionKey::from_bytes(bytes.to_vec(), KeyScope::HostLocal);
        assert!(k.key_id().starts_with("hmac-sha256:"));
        // recomputing yields the same id; the id never contains the raw key
        assert_eq!(k.key_id(), compute_key_id(&bytes));
        assert!(!k.key_id().contains(&base64url_encode(&bytes)));
    }

    #[test]
    fn generated_keys_differ() {
        assert_ne!(generate_key_bytes(), generate_key_bytes());
    }

    #[test]
    fn ephemeral_scope_and_salt() {
        let k = RedactionKey::ephemeral();
        assert_eq!(k.scope(), KeyScope::Ephemeral);
        assert_eq!(k.salt().len(), 32);
    }

    #[test]
    fn resolve_generates_then_reuses_default_file() {
        let dir = std::env::temp_dir().join(format!("assay-redact-key-{}", Uuid::new_v4()));
        let path = dir.join("redaction.key");
        // first resolve generates + persists
        let k1 = RedactionKey::resolve_host_local(None, &path).unwrap();
        assert!(path.exists());
        assert_eq!(k1.scope(), KeyScope::HostLocal);
        // second resolve reads the same key back (same key_id)
        let k2 = RedactionKey::resolve_host_local(None, &path).unwrap();
        assert_eq!(k1.key_id(), k2.key_id());
        assert_eq!(k1.salt(), k2.salt());
        let _ = fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn generated_key_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("assay-redact-perm-{}", Uuid::new_v4()));
        let path = dir.join("redaction.key");
        let _ = RedactionKey::resolve_host_local(None, &path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn env_override_takes_precedence() {
        let dir = std::env::temp_dir().join(format!("assay-redact-env-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let env_path = dir.join("ci.key");
        let bytes = generate_key_bytes();
        fs::write(&env_path, encode_key_file(&bytes)).unwrap();
        let default_path = dir.join("default.key");
        let k = RedactionKey::resolve_host_local(Some(&env_path), &default_path).unwrap();
        assert_eq!(k.salt(), bytes.to_vec());
        assert!(!default_path.exists()); // default not generated when env override is used
        let _ = fs::remove_dir_all(&dir);
    }
}
