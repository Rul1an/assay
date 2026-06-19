//! EXPERIMENTAL (unstable, may change): the observed-input side of the tool-decision truth-layer
//! carrier. It provides a keyed, domain-separated `args_digest` (raw arguments are never stored), an
//! `observed_input_digest` over the stable `{tool_name, args_digest, order}` triple, and a 3-zone
//! carrier record whose decision identity is the `(observed_input_digest, declared_policy_digest)` pair.
//! The declared side is [`super::policy::McpPolicy::declared_constraint_digest_experimental`].
//!
//! It also provides a deterministic verdict gate (tool / args / identity axes folded with the lattice
//! `invalid > mismatch > incomplete > match`) and a run-level aggregate over an ordered set of decisions.
//! Not a stability guarantee: the schema, field names, and digests may change until this is promoted out
//! of experimental.

use super::jcs;
use super::policy::{McpPolicy, UnconstrainedMode};
use crate::fingerprint::sha256_hex;
use hmac::{Hmac, KeyInit, Mac};
use serde_json::{json, Map, Value};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Experimental schema id for the carrier record.
pub const SCHEMA: &str = "assay.tool_decision_truth.v0";

/// Domain tag mixed into the keyed argument digest.
const ARGS_DIGEST_DOMAIN: &str = "assay.tool_args.v0";

/// Append-only provenance/status vocabulary for the carrier record. The carrier builder validates the
/// three free-text provenance fields against these, so an adapter cannot stamp an arbitrary
/// source/status/identity label that a consumer would then trust.
const SOURCE_CLASSES: &[&str] = &["authoritative_boundary", "reported_trace", "inferred"];
const RESULT_STATUSES: &[&str] = &["ok", "error", "n/a"];
const IDENTITY_STATES: &[&str] = &["present", "absent", "required_missing", "invalid"];

/// Secret-key tokens compared AFTER normalization (lowercase, with `_`/`-`/spaces removed). A key whose
/// normalized form equals one of these is dropped entirely before the digest (never even keyed): a token
/// or a password is not something a consumer needs to compare, so dropping is strictly safer than hashing.
/// Normalizing the key first means `Authorization`, `client_secret`, `refresh_token`, `apiKey`, and
/// `api-key` are all caught by one entry, rather than depending on an exact, case-sensitive spelling.
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

/// Normalize an argument key for secret classification: lowercase and drop `_`, `-`, and spaces, so
/// `access_token`, `accessToken`, and `Access-Token` collapse to the same token.
fn normalize_key(key: &str) -> String {
    key.chars()
        .filter(|c| *c != '_' && *c != '-' && *c != ' ')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Whether an argument key names a secret whose value must be dropped before the digest.
fn is_secret_key(key: &str) -> bool {
    SECRET_KEY_TOKENS.contains(&normalize_key(key).as_str())
}

/// The projection that feeds the argument digest: secret-like keys are dropped RECURSIVELY (at every
/// nesting level, both in objects and inside arrays), so a nested `token`/`password` never reaches the
/// digest input. Object keys are canonicalized by JCS at digest time.
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

/// Whether a `key_id` is well-formed: non-empty and drawn from `[A-Za-z0-9._-]`. The id rides verbatim in
/// the colon-delimited digest preimage and output, so an id with a colon, whitespace, or other delimiter
/// could forge or confuse the framing; rejecting out-of-charset ids keeps the digest string unambiguous.
fn is_valid_key_id(key_id: &str) -> bool {
    !key_id.is_empty()
        && key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Domain-separated, keyed argument digest. The `key_id` rides in the digest so a deployment can rotate
/// keys and a verifier can tell which key produced it. A low-entropy argument cannot be dictionary-
/// recovered from the digest by anyone without the key. The raw arguments never enter the record.
///
/// Returns `None` when the inputs cannot produce a sound, unambiguous digest: an empty `key` (which would
/// provide no privacy and emit a guessable MAC), a `key_id` outside `[A-Za-z0-9._-]` (which could confuse
/// the colon-delimited framing), or a canonicalization failure (defaulting to an empty preimage would
/// collapse distinct inputs to the same digest and break identity correctness). This is the shape of the
/// primitive; real key provisioning, rotation, and per-tool domain scoping are a later concern.
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
/// Returns `None` on canonicalization failure (never a silent empty-preimage digest).
pub fn observed_input_digest(tool_name: &str, args_digest: &str, order: i64) -> Option<String> {
    let v = json!({"tool_name": tool_name, "args_digest": args_digest, "order": order});
    let canonical = jcs::to_string(&v).ok()?;
    Some(format!("sha256:{}", sha256_hex(&canonical)))
}

/// Build a 3-zone tool-decision-truth carrier record (experimental). Returns `None` if a digest cannot
/// be canonicalized, or if `source_class`, `result_status`, or `identity_state` is outside the
/// append-only carrier vocabulary ([`SOURCE_CLASSES`] / [`RESULT_STATUSES`] / [`IDENTITY_STATES`]) — an
/// adapter cannot stamp an arbitrary provenance label that a consumer would then trust.
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
    if !SOURCE_CLASSES.contains(&source_class)
        || !RESULT_STATUSES.contains(&result_status)
        || !IDENTITY_STATES.contains(&identity_state)
    {
        return None;
    }
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

// ── EXPERIMENTAL verdict gate + run lattice ──────────────────────────────────────────────────────
// Classify an observed decision against the declared McpPolicy over independent axes (tool / args /
// identity), fold per-decision with the lattice `invalid > mismatch > incomplete > match`, and aggregate
// a run with the same lattice plus order integrity. Undecidable resolves to `incomplete`, never a silent
// pass. Mirrors the private reference-spec; unstable until promoted out of experimental.

/// Lattice rank: `invalid > mismatch > incomplete > match`. Unknown strings rank as `invalid`.
fn verdict_rank(v: &str) -> u8 {
    match v {
        "match" => 0,
        "incomplete" => 1,
        "mismatch" => 2,
        _ => 3, // "invalid" and anything unexpected
    }
}

fn to_static_verdict(v: &str) -> &'static str {
    match v {
        "match" => "match",
        "incomplete" => "incomplete",
        "mismatch" => "mismatch",
        _ => "invalid",
    }
}

fn enforcement_axis(mode: &UnconstrainedMode) -> &'static str {
    match mode {
        UnconstrainedMode::Deny => "mismatch",
        UnconstrainedMode::Allow => "match",
        UnconstrainedMode::Warn => "incomplete",
    }
}

fn tool_axis(policy: &McpPolicy, tool_name: &str) -> &'static str {
    if policy
        .tools
        .deny
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .any(|t| t == tool_name)
    {
        return "mismatch";
    }
    let allow = policy.tools.allow.as_deref().unwrap_or(&[]);
    if !allow.is_empty() {
        return if allow.iter().any(|t| t == tool_name) {
            "match"
        } else {
            "mismatch"
        };
    }
    enforcement_axis(&policy.enforcement.unconstrained_tools)
}

fn args_axis(policy: &McpPolicy, tool_name: &str, args: Option<&Value>) -> &'static str {
    let Some(args) = args else {
        return "incomplete"; // arguments not captured by the adapter
    };
    if !policy.schemas.contains_key(tool_name) {
        return enforcement_axis(&policy.enforcement.unconstrained_tools);
    }
    // v0/experimental: compiles schemas per call; a cached validator is a later optimization.
    match policy.compile_all_schemas().get(tool_name) {
        Some(validator) => {
            if validator.is_valid(args) {
                "match"
            } else {
                "mismatch"
            }
        }
        None => "incomplete", // schema present but did not compile -> cannot decide
    }
}

fn identity_axis(identity_state: &str) -> &'static str {
    match identity_state {
        "present" | "absent" => "match", // absent does NOT block match
        "required_missing" => "incomplete",
        _ => "invalid", // "invalid" (and anything unexpected) -> invalid on the identity axis
    }
}

/// Per-decision verdict over the declared policy: the lattice-max of the tool, args, and identity axes.
/// Legacy root-level allow/deny are normalized first. EXPERIMENTAL (unstable).
pub fn decision_verdict(
    policy: &McpPolicy,
    tool_name: &str,
    args: Option<&Value>,
    identity_state: &str,
) -> &'static str {
    let mut p = policy.clone();
    p.normalize_legacy_shapes();
    [
        tool_axis(&p, tool_name),
        args_axis(&p, tool_name, args),
        identity_axis(identity_state),
    ]
    .into_iter()
    .max_by_key(|&v| verdict_rank(v))
    .unwrap_or("match")
}

/// Run-level verdict: the lattice-max over per-decision verdicts, plus order integrity. Duplicate `order`
/// values make the observed sequence ambiguous and force `invalid` (a run whose order cannot be
/// established is not certified). EXPERIMENTAL (unstable).
pub fn run_verdict(decision_verdicts: &[&str], orders: &[i64]) -> &'static str {
    // Arity guard: per-decision verdicts and their orders must line up. A mismatch is untrusted input
    // (a verdict with no order, or an order with no verdict), so the run is invalid.
    if decision_verdicts.len() != orders.len() {
        return "invalid";
    }
    let mut seen = std::collections::HashSet::new();
    for o in orders {
        if !seen.insert(*o) {
            return "invalid";
        }
    }
    let mut worst: &'static str = "match";
    for v in decision_verdicts {
        let s = to_static_verdict(v);
        if verdict_rank(s) > verdict_rank(worst) {
            worst = s;
        }
    }
    worst
}

/// Build a fully-classified carrier record: the declared digest is taken from `policy`, the observed-input
/// identity from [`build_record`], and the classification zone carries the [`decision_verdict`].
/// EXPERIMENTAL. Returns `None` if a digest cannot be canonicalized.
#[allow(clippy::too_many_arguments)]
pub fn build_classified_record(
    policy: &McpPolicy,
    tool_name: &str,
    args: &Value,
    order: i64,
    key: &[u8],
    key_id: &str,
    source_class: &str,
    call_id: &str,
    result_status: &str,
    identity_state: &str,
) -> Option<Value> {
    let declared = policy.declared_constraint_digest_experimental()?;
    let mut record = build_record(
        tool_name,
        args,
        order,
        &declared,
        key,
        key_id,
        source_class,
        call_id,
        result_status,
        identity_state,
    )?;
    record["decision_verdict"] = json!(decision_verdict(
        policy,
        tool_name,
        Some(args),
        identity_state
    ));
    Some(record)
}

// ── EXPERIMENTAL pack recipe-row binding (cite-by-digest; no pack v2) ─────────────────────────────
// Bind a tool-decision-truth carrier into an Evidence Pack as a proven recipe row: an evidenceRef cites
// the carrier by its content digest, and a coherence_binding ties the row to that citation plus the run
// verdict, so a tampered carrier fails the row closed. Mirrors the private reference-spec; unstable.

/// Recipe id for the pack row (experimental).
pub const RECIPE: &str = "tool_decision_truth.v0";

/// Digest over the decision identity (the two pinned digests) — the handle a pack row cites the carrier by.
pub fn carrier_digest(observed_input_digest: &str, declared_policy_digest: &str) -> Option<String> {
    let identity = json!({
        "observed_input_digest": observed_input_digest,
        "declared_policy_digest": declared_policy_digest,
    });
    Some(format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(&identity).ok()?)
    ))
}

/// The cross-ecosystem envelope a pack row cites the carrier by: `{type, digest, canonicalization, schema, ref}`.
pub fn evidence_ref(carrier_digest: &str, reference: &str) -> Value {
    json!({
        "type": "tool_decision_truth",
        "digest": carrier_digest,
        "canonicalization": "jcs-json-v1",
        "schema": SCHEMA,
        "ref": reference,
    })
}

/// Build a proven recipe row binding the carrier into the existing Evidence Pack (no pack v2). The
/// `coherence_binding` digests the row's citation + recipe + run verdict, so a tampered carrier breaks
/// both the cited digest and the binding (fail-closed). EXPERIMENTAL. `None` on canonicalization failure.
pub fn pack_recipe_row(
    observed_input_digest: &str,
    declared_policy_digest: &str,
    run_verdict: &str,
    reference: &str,
) -> Option<Value> {
    let cd = carrier_digest(observed_input_digest, declared_policy_digest)?;
    let er = evidence_ref(&cd, reference);
    let binding_input = json!({"recipe": RECIPE, "evidence_ref": er, "run_verdict": run_verdict});
    let coherence_binding = format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(&binding_input).ok()?)
    );
    Some(json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "run_verdict": run_verdict,
        "coherence_binding": coherence_binding,
    }))
}

/// Verify a recipe row coheres with the carrier it cites: the evidenceRef digest must equal the recomputed
/// carrier digest, and the `coherence_binding` must recompute from the row's own citation + recipe +
/// verdict. A tampered carrier (different identity) or a changed verdict fails closed.
pub fn verify_recipe_row(
    row: &Value,
    observed_input_digest: &str,
    declared_policy_digest: &str,
    run_verdict: &str,
) -> bool {
    // This verifies THIS recipe and envelope, not just internal coherence: the row must declare this
    // recipe and the tool-decision-truth evidenceRef envelope (type / schema / canonicalization), else a
    // foreign row with a self-consistent binding would falsely verify.
    if row.get("recipe").and_then(|r| r.as_str()) != Some(RECIPE) {
        return false;
    }
    let Some(er) = row.get("evidence_ref").cloned() else {
        return false;
    };
    if er.get("type").and_then(|x| x.as_str()) != Some("tool_decision_truth")
        || er.get("schema").and_then(|x| x.as_str()) != Some(SCHEMA)
        || er.get("canonicalization").and_then(|x| x.as_str()) != Some("jcs-json-v1")
    {
        return false;
    }
    let Some(cd) = carrier_digest(observed_input_digest, declared_policy_digest) else {
        return false;
    };
    if er.get("digest").and_then(|d| d.as_str()) != Some(cd.as_str()) {
        return false;
    }
    let binding_input = json!({"recipe": RECIPE, "evidence_ref": er, "run_verdict": run_verdict});
    let expected = match jcs::to_string(&binding_input) {
        Ok(s) => format!("sha256:{}", sha256_hex(&s)),
        Err(_) => return false,
    };
    row.get("coherence_binding").and_then(|c| c.as_str()) == Some(expected.as_str())
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
    fn secret_classifier_catches_case_and_separator_variants() {
        // The canonical secret keys, plus camelCase, kebab-case, and UPPER variants, are all dropped, so
        // changing only a secret-named value never moves the digest.
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
        // A field that is not a secret (even if it shares letters with one) is kept, so a real change
        // moves the digest.
        assert_ne!(ad(json!({"account": "a"})), ad(json!({"account": "b"})));
    }

    #[test]
    fn args_digest_rejects_empty_key_and_bad_key_id() {
        assert!(args_digest(&json!({"path": "/x"}), b"", KID).is_none());
        assert!(args_digest(&json!({"path": "/x"}), KEY, "").is_none());
        assert!(args_digest(&json!({"path": "/x"}), KEY, "bad id").is_none()); // whitespace
        assert!(args_digest(&json!({"path": "/x"}), KEY, "bad:id").is_none()); // framing delimiter
        assert!(args_digest(&json!({"path": "/x"}), KEY, "good.key_id-v0").is_some());
    }

    #[test]
    fn build_record_rejects_out_of_contract_provenance() {
        let args = json!({"path": "/x"});
        let mk = |sc: &str, rs: &str, id: &str| {
            build_record(
                "read_file",
                &args,
                0,
                "sha256:d",
                KEY,
                KID,
                sc,
                "c1",
                rs,
                id,
            )
        };
        assert!(mk("authoritative_boundary", "ok", "present").is_some());
        assert!(mk("made_up_source", "ok", "present").is_none());
        assert!(mk("authoritative_boundary", "maybe", "present").is_none());
        assert!(mk("authoritative_boundary", "ok", "unknown_state").is_none());
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

#[cfg(test)]
mod gate_tests {
    use super::*;

    const KEY: &[u8] = b"k";
    const KID: &str = "kid";

    fn policy() -> McpPolicy {
        serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["read_file", "deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap()
    }

    fn v(tool: &str, args: Option<Value>, id: &str) -> &'static str {
        decision_verdict(&policy(), tool, args.as_ref(), id)
    }

    #[test]
    fn per_decision_verdict_matrix() {
        // allowed tool, args satisfy the schema, identity present -> match
        assert_eq!(
            v("deploy", Some(json!({"env": "staging"})), "present"),
            "match"
        );
        // denied tool -> mismatch
        assert_eq!(v("delete_all", Some(json!({})), "present"), "mismatch");
        // tool absent from a non-empty allow-list -> mismatch
        assert_eq!(v("exfiltrate", Some(json!({})), "present"), "mismatch");
        // allowed tool, arg violates the enum -> mismatch
        assert_eq!(
            v("deploy", Some(json!({"env": "dev"})), "present"),
            "mismatch"
        );
        // allowed tool, args not captured -> incomplete
        assert_eq!(v("deploy", None, "present"), "incomplete");
        // allowed tool with no schema under enforcement=warn -> incomplete
        assert_eq!(
            v("read_file", Some(json!({"path": "/x"})), "present"),
            "incomplete"
        );
        // identity required but missing -> incomplete
        assert_eq!(
            v(
                "deploy",
                Some(json!({"env": "staging"})),
                "required_missing"
            ),
            "incomplete"
        );
        // identity present but invalid -> invalid
        assert_eq!(
            v("deploy", Some(json!({"env": "staging"})), "invalid"),
            "invalid"
        );
    }

    #[test]
    fn absent_identity_does_not_block_match() {
        assert_eq!(v("deploy", Some(json!({"env": "prod"})), "absent"), "match");
    }

    #[test]
    fn run_lattice_and_order_integrity() {
        assert_eq!(run_verdict(&["match", "incomplete"], &[0, 1]), "incomplete");
        assert_eq!(run_verdict(&["match", "mismatch"], &[0, 1]), "mismatch");
        assert_eq!(
            run_verdict(&["match", "mismatch", "invalid"], &[0, 1, 2]),
            "invalid"
        );
        assert_eq!(run_verdict(&["match", "match"], &[0, 0]), "invalid"); // duplicate order
        assert_eq!(run_verdict(&["match"], &[0]), "match");
        // arity mismatch between verdicts and orders -> invalid
        assert_eq!(run_verdict(&["match", "match"], &[0]), "invalid");
        assert_eq!(run_verdict(&["match"], &[0, 1]), "invalid");
    }

    #[test]
    fn build_classified_record_carries_the_verdict() {
        let m = build_classified_record(
            &policy(),
            "deploy",
            &json!({"env": "staging"}),
            0,
            KEY,
            KID,
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
        )
        .unwrap();
        assert_eq!(m["decision_verdict"], json!("match"));
        let mm = build_classified_record(
            &policy(),
            "delete_all",
            &json!({}),
            0,
            KEY,
            KID,
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
        )
        .unwrap();
        assert_eq!(mm["decision_verdict"], json!("mismatch"));
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;

    const OID: &str = "sha256:observed";
    const DPD: &str = "sha256:declared";
    const REF: &str = "audit://decision/c1";

    #[test]
    fn evidence_ref_has_the_five_envelope_fields() {
        let cd = carrier_digest(OID, DPD).unwrap();
        let er = evidence_ref(&cd, REF);
        let mut keys: Vec<&String> = er.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec!["canonicalization", "digest", "ref", "schema", "type"]
        );
        assert_eq!(er["schema"], json!(SCHEMA));
        assert_eq!(er["digest"], json!(cd));
    }

    #[test]
    fn recipe_row_coheres_with_its_carrier() {
        let row = pack_recipe_row(OID, DPD, "match", REF).unwrap();
        assert!(verify_recipe_row(&row, OID, DPD, "match"));
    }

    #[test]
    fn tampered_carrier_or_verdict_fails_closed() {
        let row = pack_recipe_row(OID, DPD, "match", REF).unwrap();
        // A different carrier identity (tampered observed/declared digest) must not cohere.
        assert!(!verify_recipe_row(&row, "sha256:TAMPERED", DPD, "match"));
        assert!(!verify_recipe_row(&row, OID, "sha256:TAMPERED", "match"));
        // A changed run verdict must not cohere (the coherence_binding covers it).
        assert!(!verify_recipe_row(&row, OID, DPD, "mismatch"));
    }

    /// A row whose coherence_binding is self-consistent over the given (possibly foreign) recipe/envelope.
    fn coherent_row(recipe: &str, er: Value, run_verdict: &str) -> Value {
        let binding_input =
            json!({"recipe": recipe, "evidence_ref": er, "run_verdict": run_verdict});
        let cb = format!(
            "sha256:{}",
            crate::fingerprint::sha256_hex(&crate::mcp::jcs::to_string(&binding_input).unwrap())
        );
        json!({"recipe": recipe, "evidence_ref": er, "run_verdict": run_verdict, "coherence_binding": cb})
    }

    #[test]
    fn verify_rejects_foreign_recipe_or_envelope_even_when_self_coherent() {
        let cd = carrier_digest(OID, DPD).unwrap();
        let good_er = evidence_ref(&cd, REF);
        // Sanity: the proper recipe + envelope verifies.
        assert!(verify_recipe_row(
            &coherent_row(RECIPE, good_er.clone(), "match"),
            OID,
            DPD,
            "match"
        ));
        // A self-coherent row with a FOREIGN recipe is rejected.
        assert!(!verify_recipe_row(
            &coherent_row("other.recipe.v0", good_er.clone(), "match"),
            OID,
            DPD,
            "match"
        ));
        // Foreign envelope type / schema / canonicalization are each rejected.
        for (field, bad) in [
            ("type", json!("other")),
            ("schema", json!("other.schema.v0")),
            ("canonicalization", json!("cbor-deterministic-v1")),
        ] {
            let mut er = good_er.as_object().unwrap().clone();
            er.insert(field.to_string(), bad);
            let row = coherent_row(RECIPE, Value::Object(er), "match");
            assert!(
                !verify_recipe_row(&row, OID, DPD, "match"),
                "field {field} must be rejected"
            );
        }
    }
}
