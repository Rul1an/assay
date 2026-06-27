use crate::fingerprint::sha256_hex;
use crate::mcp::jcs;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Map, Value};
use sha2::Sha256;

use super::{is_sha256_digest, SCHEMA};

type HmacSha256 = Hmac<Sha256>;

/// Domain tag mixed into the keyed argument digest.
const ARGS_DIGEST_DOMAIN: &str = "assay.tool_args.v0";

/// Append-only provenance/status vocabulary for the carrier record. The carrier builder validates the
/// three free-text provenance fields against these, so an adapter cannot stamp arbitrary labels.
const SOURCE_CLASSES: &[&str] = &["authoritative_boundary", "reported_trace", "inferred"];
const RESULT_STATUSES: &[&str] = &["ok", "error", "n/a"];
const IDENTITY_STATES: &[&str] = &["present", "absent", "required_missing", "invalid"];

/// Secret-key tokens compared AFTER normalization (lowercase, with `_`/`-`/spaces removed). A key whose
/// normalized form equals one of these is dropped entirely before the digest.
const SECRET_KEY_TOKENS: &[&str] = &[
    "token",
    "accesstoken",
    "refreshtoken",
    "authtoken",
    "idtoken",
    "sessiontoken",
    "password",
    "passwd",
    "apikey",
    "secret",
    "clientsecret",
    "secretkey",
    "authorization",
    "credential",
    "credentials",
    "privatekey",
    "secretaccesskey",
];

/// Normalize an argument key for secret classification: lowercase and drop `_`, `-`, and spaces.
fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|c| *c != '_' && *c != '-' && *c != ' ')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn is_secret_key(key: &str) -> bool {
    SECRET_KEY_TOKENS.contains(&normalize_key(key).as_str())
}

/// The projection that feeds the argument digest: secret-like keys are dropped recursively at every
/// nesting level. Object keys are canonicalized by JCS at digest time.
fn project_args_for_digest(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut out = Map::new();
            for (k, v) in obj {
                if is_secret_key(k) {
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

/// Whether a `key_id` is well-formed: non-empty and drawn from `[A-Za-z0-9._-]`.
fn is_valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Domain-separated, keyed argument digest. The `key_id` rides in the digest so a deployment can rotate
/// keys and a verifier can tell which key produced it. The raw arguments never enter the record.
pub fn args_digest(args: &Value, key: &[u8], key_id: &str) -> Option<String> {
    if key.is_empty() || !is_valid_key_id(key_id) {
        return None;
    }
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
pub fn observed_input_digest(tool_name: &str, args_digest: &str, order: i64) -> Option<String> {
    let v = json!({"tool_name": tool_name, "args_digest": args_digest, "order": order});
    let canonical = jcs::to_string(&v).ok()?;
    Some(format!("sha256:{}", sha256_hex(&canonical)))
}

/// Build a 3-zone tool-decision-truth carrier record (experimental). Returns `None` if a digest cannot
/// be canonicalized, or if provenance/status fields are outside the append-only vocabulary.
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
    if !SOURCE_CLASSES.contains(&source_class)
        || !RESULT_STATUSES.contains(&result_status)
        || !IDENTITY_STATES.contains(&identity_state)
    {
        return None;
    }
    if !is_sha256_digest(declared_policy_digest) {
        return None;
    }
    let ad = args_digest(args, key, key_id)?;
    let oid = observed_input_digest(tool_name, &ad, order)?;
    Some(json!({
        "schema": SCHEMA,
        "tool_name": tool_name,
        "args_digest": ad,
        "order": order,
        "source_class": source_class,
        "call_id": call_id,
        "result_status": result_status,
        "identity_state": identity_state,
        "key_id": key_id,
        "declared_ref": Value::Null,
        "decision_verdict": Value::Null,
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
    const DECL: &str = "sha256:1111111111111111111111111111111111111111111111111111111111111111";

    fn ad(args: Value) -> String {
        args_digest(&args, KEY, KID).unwrap()
    }

    fn rec(tool: &str, args: Value, order: i64, prov: (&str, &str, &str, &str)) -> Value {
        build_record(
            tool, &args, order, DECL, KEY, KID, prov.0, prov.1, prov.2, prov.3,
        )
        .unwrap()
    }

    #[test]
    fn args_digest_drops_secret_fields_recursively() {
        assert_eq!(
            ad(json!({"path": "/x", "token": "aaa"})),
            ad(json!({"path": "/x", "token": "bbb"}))
        );
        assert_eq!(
            ad(json!({"path": "/x", "token": "aaa"})),
            ad(json!({"path": "/x"}))
        );
        assert_eq!(
            ad(json!({"cfg": {"host": "h", "token": "aaa"}})),
            ad(json!({"cfg": {"host": "h", "token": "bbb"}}))
        );
        assert_eq!(
            ad(json!({"cfg": {"host": "h", "token": "aaa"}})),
            ad(json!({"cfg": {"host": "h"}}))
        );
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
    fn secret_classifier_catches_case_and_separator_variants() {
        let base = ad(json!({"path": "/x"}));
        for secret in [
            "Authorization",
            "authorization",
            "client_secret",
            "clientSecret",
            "refresh_token",
            "refreshToken",
            "ACCESS_TOKEN",
            "access-token",
            "apiKey",
            "api-key",
            "privateKey",
            "secretAccessKey",
        ] {
            let mut withv = serde_json::Map::new();
            withv.insert("path".into(), json!("/x"));
            withv.insert(secret.into(), json!("super-secret-value"));
            assert_eq!(
                ad(Value::Object(withv)),
                base,
                "secret-named key `{secret}` must be dropped before the digest"
            );
        }
    }

    #[test]
    fn non_secret_key_still_contributes() {
        assert_ne!(ad(json!({"account": "a"})), ad(json!({"account": "b"})));
    }

    #[test]
    fn args_digest_rejects_empty_key_and_bad_key_id() {
        assert!(args_digest(&json!({"path": "/x"}), b"", KID).is_none());
        assert!(args_digest(&json!({"path": "/x"}), KEY, "").is_none());
        assert!(args_digest(&json!({"path": "/x"}), KEY, "bad id").is_none());
        assert!(args_digest(&json!({"path": "/x"}), KEY, "bad:id").is_none());
        assert!(args_digest(&json!({"path": "/x"}), KEY, "good.key_id-v0").is_some());
    }

    #[test]
    fn build_record_rejects_out_of_contract_provenance() {
        let args = json!({"path": "/x"});
        let mk = |sc: &str, rs: &str, id: &str| {
            build_record("read_file", &args, 0, DECL, KEY, KID, sc, "c1", rs, id)
        };
        assert!(mk("authoritative_boundary", "ok", "present").is_some());
        assert!(mk("made_up_source", "ok", "present").is_none());
        assert!(mk("authoritative_boundary", "maybe", "present").is_none());
        assert!(mk("authoritative_boundary", "ok", "unknown_state").is_none());
    }

    #[test]
    fn build_record_rejects_malformed_declared_digest() {
        let args = json!({"path": "/x"});
        let mk = |decl: &str| {
            build_record(
                "read_file",
                &args,
                0,
                decl,
                KEY,
                KID,
                "authoritative_boundary",
                "c1",
                "ok",
                "present",
            )
        };
        assert!(mk(DECL).is_some());
        assert!(mk("sha256:decl").is_none());
        assert!(mk("not-a-digest").is_none());
    }

    #[test]
    fn low_entropy_arg_not_recoverable_without_the_key() {
        let truth = args_digest(&json!({"admin": true}), KEY, KID);
        let space = [json!({"admin": true}), json!({"admin": false})];
        assert!(space.iter().any(|c| args_digest(c, KEY, KID) == truth));
        assert!(!space
            .iter()
            .any(|c| args_digest(c, b"attacker-guess", KID) == truth));
    }

    #[test]
    fn identity_stable_under_provenance_changes() {
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
