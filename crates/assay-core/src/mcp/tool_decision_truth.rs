//! EXPERIMENTAL (unstable, may change): the observed-input side of the tool-decision truth-layer
//! carrier. It provides a keyed, domain-separated `args_digest` (raw arguments are never stored), an
//! `observed_input_digest` over the stable `{tool_name, args_digest, order}` triple, and a 3-zone
//! carrier record whose decision identity is the `(observed_input_digest, declared_policy_digest)` pair.
//! The declared side is [`super::policy::McpPolicy::declared_constraint_digest_experimental`].
//!
//! This slice carries the record shape only; no verdict is computed here (a later slice owns the gate),
//! so the classification zone is carried but not decided. Not a stability guarantee: the schema, field
//! names, and digests may change until this is promoted out of experimental.

use super::jcs;
use crate::fingerprint::sha256_hex;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Map, Value};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Experimental schema id for the carrier record.
pub const SCHEMA: &str = "assay.tool_decision_truth.v0";

/// Domain tag mixed into the keyed argument digest.
const ARGS_DIGEST_DOMAIN: &str = "assay.tool_args.v0";

/// Argument keys whose values are dropped entirely before the digest (never even keyed): a token or a
/// password is not something a consumer needs to compare, so dropping is strictly safer than hashing.
const SECRET_DROP: &[&str] = &[
    "token",
    "access_token",
    "password",
    "api_key",
    "secret",
    "authorization",
    "private_key",
];

/// The projection that feeds the argument digest: secret-like keys are dropped RECURSIVELY (at every
/// nesting level, both in objects and inside arrays), so a nested `token`/`password` never reaches the
/// digest input. Object keys are canonicalized by JCS at digest time.
fn project_args_for_digest(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut out = Map::new();
            for (k, v) in obj {
                if SECRET_DROP.contains(&k.as_str()) {
                    continue;
                }
                out.insert(k.clone(), project_args_for_digest(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(project_args_for_digest).collect()),
        other => other.clone(),
    }
}

/// Domain-separated, keyed argument digest. The `key_id` rides in the digest so a deployment can rotate
/// keys and a verifier can tell which key produced it. A low-entropy argument cannot be dictionary-
/// recovered from the digest by anyone without the key. The raw arguments never enter the record.
///
/// Returns `None` on canonicalization failure rather than defaulting to an empty preimage (which would
/// collapse distinct inputs to the same digest and break identity correctness). This is the shape of the
/// primitive; real key provisioning, rotation, and per-tool domain scoping are a later concern.
pub fn args_digest(args: &Value, key: &[u8], key_id: &str) -> Option<String> {
    let proj = project_args_for_digest(args);
    let canonical = jcs::to_string(&proj).ok()?;
    let preimage = format!("{ARGS_DIGEST_DOMAIN}:{key_id}:{canonical}");
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts a key of any length");
    mac.update(preimage.as_bytes());
    Some(format!(
        "hmac-sha256:{key_id}:{}",
        hex::encode(mac.finalize().into_bytes())
    ))
}

/// Digest over the stable observed-input identity triple only (`tool_name`, `args_digest`, `order`).
/// Returns `None` on canonicalization failure (never a silent empty-preimage digest).
pub fn observed_input_digest(tool_name: &str, args_digest: &str, order: i64) -> Option<String> {
    let v = json!({"tool_name": tool_name, "args_digest": args_digest, "order": order});
    let canonical = jcs::to_string(&v).ok()?;
    Some(format!("sha256:{}", sha256_hex(&canonical)))
}

/// Build a 3-zone tool-decision-truth carrier record (experimental). Returns `None` if a digest cannot
/// be canonicalized.
///
/// Zone (i) observed-input identity is the only thing inside `observed_input_digest`. Zone (ii)
/// observation provenance and zone (iii) classification output are recorded but excluded from the
/// identity, so adapter labels and a later verdict never perturb replay. `decision_identity` is the
/// `(observed_input_digest, declared_policy_digest)` pair; the declared digest is supplied by the caller
/// (from `McpPolicy::declared_constraint_digest_experimental`). No verdict is decided here.
#[allow(clippy::too_many_arguments)]
pub fn build_record(
    tool_name: &str,
    args: &Value,
    order: i64,
    declared_policy_digest: &str,
    key: &[u8],
    key_id: &str,
    source_class: &str,
    call_id: &str,
    result_status: &str,
    identity_state: &str,
) -> Option<Value> {
    let ad = args_digest(args, key, key_id)?;
    let oid = observed_input_digest(tool_name, &ad, order)?;
    Some(json!({
        "schema": SCHEMA,
        // (i) observed-input identity — the ONLY fields inside observed_input_digest
        "tool_name": tool_name,
        "args_digest": ad,
        "order": order,
        // (ii) observation provenance — recorded, excluded from the identity digest
        "source_class": source_class,
        "call_id": call_id,
        "result_status": result_status,
        "identity_state": identity_state,
        "key_id": key_id,
        // (iii) classification output — owned by a later verdict slice; carried, excluded from identity
        "declared_ref": Value::Null,
        "decision_verdict": Value::Null,
        // digest-bound identity
        "observed_input_digest": oid,
        "declared_policy_digest": declared_policy_digest,
        "decision_identity": {
            "observed_input_digest": oid,
            "declared_policy_digest": declared_policy_digest,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] = b"reference-test-key-v0";
    const KID: &str = "test-key-v0";
    const PROV: (&str, &str, &str, &str) = ("authoritative_boundary", "c1", "ok", "present");

    fn ad(args: Value) -> String {
        args_digest(&args, KEY, KID).unwrap()
    }

    fn rec(tool: &str, args: Value, order: i64, prov: (&str, &str, &str, &str)) -> Value {
        build_record(
            tool,
            &args,
            order,
            "sha256:decl",
            KEY,
            KID,
            prov.0,
            prov.1,
            prov.2,
            prov.3,
        )
        .unwrap()
    }

    #[test]
    fn args_digest_drops_secret_fields_recursively() {
        // SECRET_DROP keys never affect the digest, at the top level AND nested in objects/arrays.
        assert_eq!(
            ad(json!({"path": "/x", "token": "aaa"})),
            ad(json!({"path": "/x", "token": "bbb"}))
        );
        assert_eq!(
            ad(json!({"path": "/x", "token": "aaa"})),
            ad(json!({"path": "/x"}))
        );
        // nested object:
        assert_eq!(
            ad(json!({"cfg": {"host": "h", "token": "aaa"}})),
            ad(json!({"cfg": {"host": "h", "token": "bbb"}}))
        );
        assert_eq!(
            ad(json!({"cfg": {"host": "h", "token": "aaa"}})),
            ad(json!({"cfg": {"host": "h"}}))
        );
        // inside an array:
        assert_eq!(
            ad(json!({"items": [{"password": "p1"}, {"k": "v"}]})),
            ad(json!({"items": [{"password": "p2"}, {"k": "v"}]}))
        );
    }

    #[test]
    fn args_digest_carries_key_id() {
        assert!(ad(json!({"path": "/x"})).starts_with("hmac-sha256:test-key-v0:"));
    }

    #[test]
    fn low_entropy_arg_not_recoverable_without_the_key() {
        // The true value matches WITH the key; an attacker without the key cannot reproduce the digest
        // even over the tiny value space.
        let truth = args_digest(&json!({"admin": true}), KEY, KID);
        let space = [json!({"admin": true}), json!({"admin": false})];
        assert!(space.iter().any(|c| args_digest(c, KEY, KID) == truth));
        assert!(!space
            .iter()
            .any(|c| args_digest(c, b"attacker-guess", KID) == truth));
    }

    #[test]
    fn identity_stable_under_provenance_changes() {
        // Same observed inputs, different provenance / labels => identical identity (replay-stable).
        let a = rec("read_file", json!({"path": "/a"}), 0, PROV);
        let b = rec(
            "read_file",
            json!({"path": "/a"}),
            0,
            ("reported_trace", "c2", "error", "absent"),
        );
        assert_eq!(a["observed_input_digest"], b["observed_input_digest"]);
        assert_eq!(a["decision_identity"], b["decision_identity"]);
    }

    #[test]
    fn identity_changes_with_observed_inputs() {
        let base = rec("read_file", json!({"path": "/a"}), 0, PROV);
        let oid = &base["observed_input_digest"];
        assert_ne!(
            oid,
            &rec("list_dir", json!({"path": "/a"}), 0, PROV)["observed_input_digest"]
        );
        assert_ne!(
            oid,
            &rec("read_file", json!({"path": "/b"}), 0, PROV)["observed_input_digest"]
        );
        assert_ne!(
            oid,
            &rec("read_file", json!({"path": "/a"}), 5, PROV)["observed_input_digest"]
        );
    }

    #[test]
    fn record_never_carries_raw_args() {
        let r = rec(
            "read_file",
            json!({"path": "/a", "token": "secret"}),
            0,
            PROV,
        );
        assert!(r.get("args").is_none() && r.get("arguments").is_none());
        assert!(r["args_digest"]
            .as_str()
            .unwrap()
            .starts_with("hmac-sha256:"));
    }

    #[test]
    fn decision_identity_is_the_two_digests_only() {
        let r = rec("read_file", json!({"path": "/a"}), 0, PROV);
        let di = r["decision_identity"].as_object().unwrap();
        let mut keys: Vec<&String> = di.keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["declared_policy_digest", "observed_input_digest"]
        );
    }
}
