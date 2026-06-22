//! Profile-bound semantic digests.
//!
//! The semantic-digest contract is not just "JCS + sets": it is JCS, a specific set-path registry,
//! and the reject rules, named by a `canonicalization_profile` string. [`semantic_digest`] binds
//! that profile into the digest preimage, so:
//!
//! - a digest computed under one profile never collides with one computed under another, and
//! - introducing (or bumping) the profile moves the digest with no reorder — the contract's
//!   "old/new golden pair" on first adoption — instead of silently changing meaning.
//!
//! A consumer reads the profile a record was produced under and calls [`ensure_supported_profile`]
//! to fail closed on anything this build does not implement, rather than recompute under the latest
//! rules.

use serde::Serialize;
use serde_json::Value;

use crate::set_paths::{normalize_sets, SetPath};
use crate::{content_id, Error};

/// The canonicalization profile this crate implements: RFC 8785 (JCS) bytes + the semantic set-path
/// registry + sha256 content-addressing. Bound into every [`semantic_digest`].
pub const PROFILE: &str = "assay.semantic-digest.jcs-rfc8785.v1";

/// The v1 preimage shape: the profile string alongside the normalized record. JCS sorts the two
/// keys deterministically, so the profile is unambiguously part of the bytes that are hashed.
#[derive(Serialize)]
struct Profiled<'a> {
    canonicalization_profile: &'a str,
    record: &'a Value,
}

/// Compute the profile-bound semantic digest of `record`.
///
/// Normalizes the registered `set_paths`, binds `profile` into the preimage, then content-addresses.
/// `profile` is a required argument (never defaulted) so a caller cannot omit it; pass [`PROFILE`]
/// unless deliberately producing a digest under a different, named profile.
///
/// ```
/// use serde_json::json;
/// use assay_canonical::{semantic_digest, PROFILE};
///
/// let paths = vec![vec!["passed_keys".to_string()]];
/// let a = semantic_digest(&json!({"passed_keys": ["B", "A"]}), &paths, PROFILE).unwrap();
/// let b = semantic_digest(&json!({"passed_keys": ["A", "B"]}), &paths, PROFILE).unwrap();
/// assert_eq!(a, b); // set order does not change the digest
/// assert!(a.starts_with("sha256:"));
/// ```
pub fn semantic_digest(
    record: &Value,
    set_paths: &[SetPath],
    profile: &str,
) -> Result<String, Error> {
    let normalized = normalize_sets(record, set_paths)?;
    content_id(&Profiled {
        canonicalization_profile: profile,
        record: &normalized,
    })
}

/// Accept `profile` only if this build implements it; otherwise return [`Error::UnknownProfile`].
///
/// A consumer calls this with the profile read from a record *before* recomputing its digest, so an
/// unknown or newer profile fails closed instead of being silently recomputed under current rules.
pub fn ensure_supported_profile(profile: &str) -> Result<(), Error> {
    if profile == PROFILE {
        Ok(())
    } else {
        Err(Error::UnknownProfile(profile.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn env_paths() -> Vec<SetPath> {
        vec![vec!["passed_keys".to_string()]]
    }

    #[test]
    fn profile_is_bound_into_the_preimage() {
        // The profiled digest must differ from the bare content_id of the same normalized record,
        // i.e. the profile is actually in the bytes that are hashed.
        let rec = json!({"passed_keys": ["B", "A"]});
        let normalized = normalize_sets(&rec, &env_paths()).unwrap();
        let bare = content_id(&normalized).unwrap();
        let profiled = semantic_digest(&rec, &env_paths(), PROFILE).unwrap();
        assert_ne!(bare, profiled, "binding the profile must move the digest");
    }

    #[test]
    fn changing_the_profile_changes_the_digest() {
        let rec = json!({"passed_keys": ["A"]});
        let v1 = semantic_digest(&rec, &env_paths(), PROFILE).unwrap();
        let v2 =
            semantic_digest(&rec, &env_paths(), "assay.semantic-digest.jcs-rfc8785.v2").unwrap();
        assert_ne!(v1, v2);
    }

    #[test]
    fn set_order_does_not_change_the_digest() {
        let a =
            semantic_digest(&json!({"passed_keys": ["A", "B"]}), &env_paths(), PROFILE).unwrap();
        let b =
            semantic_digest(&json!({"passed_keys": ["B", "A"]}), &env_paths(), PROFILE).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn unknown_profile_is_rejectable() {
        assert!(ensure_supported_profile(PROFILE).is_ok());
        assert!(matches!(
            ensure_supported_profile("something-else.v9"),
            Err(Error::UnknownProfile(_))
        ));
    }

    #[test]
    fn malformed_set_propagates_as_typed_error() {
        let err = semantic_digest(&json!({"passed_keys": [1]}), &env_paths(), PROFILE).unwrap_err();
        assert!(matches!(err, Error::SetPath(_)));
    }
}
