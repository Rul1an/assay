//! EXPERIMENTAL (unstable, may change): the observed-input side of the tool-decision truth-layer
//! carrier. It provides a keyed, domain-separated `args_digest` (raw arguments are never stored), an
//! `observed_input_digest` over the stable `{tool_name, args_digest, order}` triple, and a 3-zone
//! carrier record whose decision identity is the `(observed_input_digest, declared_policy_digest)` pair.
//! The declared side is [`super::policy::McpPolicy::declared_constraint_digest_experimental`].
//!
//! It also provides a deterministic verdict gate over every axis the declared digest binds (tool name,
//! args schema, identity, classes, approval, scope, redaction), folded with the lattice
//! `invalid > mismatch > incomplete > match`, plus a run-level aggregate over an ordered set of decisions.
//! A declared constraint the gate cannot yet evaluate resolves to `incomplete`, so `match` never silently
//! means "the subset we checked matched". Not a stability guarantee: the schema, field names, and digests
//! may change until this is promoted out of experimental.

use super::jcs;
use super::policy::{ArgsCheck, McpPolicy, UnconstrainedMode};
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
    // The declared digest is supplied by the caller; reject a malformed one so a carrier cannot embed a
    // bogus `declared_policy_digest` that later passes pack verification.
    if !is_sha256_digest(declared_policy_digest) {
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
// Classify an observed decision against the declared McpPolicy over independent axes (tool name, args
// schema, identity, classes, approval, scope, redaction), fold per-decision with the lattice
// `invalid > mismatch > incomplete > match`, and aggregate a run with the same lattice plus order
// integrity. A declared constraint that cannot be evaluated with the supplied evidence resolves to
// `incomplete`, never a silent pass. Mirrors the private reference-spec; unstable until promoted out of
// experimental.

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

fn nonempty(list: &Option<Vec<String>>) -> &[String] {
    list.as_deref().unwrap_or(&[])
}

fn tool_axis(policy: &McpPolicy, tool_name: &str) -> &'static str {
    // Reuse the engine's own pattern semantics (`*`, prefix, suffix, infix, exact) so the gate does not
    // drift from how the real policy engine matches names. A literal `delete_*` deny now blocks
    // `delete_all`, instead of slipping through because the gate used exact equality.
    let matches = |p: &String| McpPolicy::tool_name_matches_experimental(tool_name, p);
    if nonempty(&policy.tools.deny).iter().any(matches) {
        return "mismatch";
    }
    let allow = nonempty(&policy.tools.allow);
    if !allow.is_empty() {
        return if allow.iter().any(matches) {
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
    // v0/experimental: compiles the declared schema per call (a cached validator is a later
    // optimization) and, crucially, does NOT panic on a malformed declared schema.
    match policy.check_tool_args_experimental(tool_name, args) {
        ArgsCheck::NoSchema => enforcement_axis(&policy.enforcement.unconstrained_tools),
        ArgsCheck::Valid => "match",
        ArgsCheck::Invalid => "mismatch",
        // A declared schema that does not compile is an invalid declaration, not missing evidence.
        ArgsCheck::Malformed => "invalid",
    }
}

fn identity_axis(identity_state: &str) -> &'static str {
    match identity_state {
        "present" | "absent" => "match", // absent does NOT block match
        "required_missing" => "incomplete",
        _ => "invalid", // "invalid" (and anything unexpected) -> invalid on the identity axis
    }
}

/// Class allow/deny axis. Not declared -> not applicable (`match`). Declared but no tool-class evidence ->
/// `incomplete`. Otherwise a denied class -> `mismatch`, and a non-empty allow-list the tool's classes
/// miss -> `mismatch`.
fn class_axis(policy: &McpPolicy, tool_classes: Option<&[String]>) -> &'static str {
    let deny_c = nonempty(&policy.tools.deny_classes);
    let allow_c = nonempty(&policy.tools.allow_classes);
    if deny_c.is_empty() && allow_c.is_empty() {
        return "match"; // not declared -> not applicable to this decision
    }
    let Some(tc) = tool_classes else {
        return "incomplete"; // declared but the tool's class membership was not supplied
    };
    if !deny_c.is_empty() && tc.iter().any(|c| deny_c.contains(c)) {
        return "mismatch";
    }
    if !allow_c.is_empty() && !tc.iter().any(|c| allow_c.contains(c)) {
        return "mismatch";
    }
    "match"
}

/// Whether a name/class-scoped obligation (approval, scope, redaction) applies to this decision.
enum Applicability {
    Applicable,
    NotApplicable,
    /// Class-scoped, but the tool's class membership was not supplied, so applicability is undecidable.
    Undeterminable,
}

fn applicability(
    names: &[String],
    classes: &[String],
    tool_name: &str,
    tool_classes: Option<&[String]>,
) -> Applicability {
    if names
        .iter()
        .any(|p| McpPolicy::tool_name_matches_experimental(tool_name, p))
    {
        return Applicability::Applicable;
    }
    if classes.is_empty() {
        return Applicability::NotApplicable; // only name-scoped, and no name matched
    }
    match tool_classes {
        None => Applicability::Undeterminable,
        Some(tc) if tc.iter().any(|c| classes.contains(c)) => Applicability::Applicable,
        Some(_) => Applicability::NotApplicable,
    }
}

/// Obligation axis (approval / scope / redaction): not applicable -> `match`; applicable but no evidence
/// it was satisfied -> `incomplete`; applicable and satisfied -> `match`; applicable and not satisfied ->
/// `mismatch`. This is the discipline that "not evaluated yet" never silently becomes `match`.
fn obligation_axis(applic: Applicability, satisfied: Option<bool>) -> &'static str {
    match applic {
        Applicability::NotApplicable => "match",
        Applicability::Undeterminable => "incomplete",
        Applicability::Applicable => match satisfied {
            None => "incomplete",
            Some(true) => "match",
            Some(false) => "mismatch",
        },
    }
}

/// EXPERIMENTAL: runtime evidence the verdict gate needs to decide declared constraints beyond the tool
/// name, args schema, and identity. Every field is optional; an absent field for a DECLARED constraint
/// resolves that axis to `incomplete`, never a silent `match`. v0 carries the evidence explicitly so a
/// caller sees exactly what a `match` requires; richer evaluators (deriving classes from a taxonomy,
/// validating approval artifacts and scope/redaction results) are future work.
#[derive(Debug, Clone, Default)]
pub struct DecisionEvidence {
    /// The observed tool's class memberships, for `allow_classes`/`deny_classes` and class-scoped
    /// approval/scope/redaction applicability.
    pub tool_classes: Option<Vec<String>>,
    /// Whether the required approval was obtained for this call.
    pub approval_obtained: Option<bool>,
    /// Whether the declared scope restriction was satisfied for this call.
    pub scope_satisfied: Option<bool>,
    /// Whether the declared argument redaction was applied for this call.
    pub redaction_applied: Option<bool>,
}

/// Per-decision verdict over the declared policy: the lattice-max of every axis the declared digest binds
/// (tool name, args schema, identity, classes, approval, scope, redaction). The cardinal rule is that a
/// declared constraint the gate cannot yet evaluate resolves to `incomplete`, so `match` means "every
/// declared constraint relevant to this decision was evaluated or proven not applicable", never "the
/// subset the gate knows about matched". Legacy root-level allow/deny are normalized first. EXPERIMENTAL.
pub fn decision_verdict(
    policy: &McpPolicy,
    tool_name: &str,
    args: Option<&Value>,
    identity_state: &str,
    evidence: &DecisionEvidence,
) -> &'static str {
    // Use the SAME normalized+migrated view as the declared digest, so a legacy-constraint-only policy
    // (whose constraint the digest binds as a migrated schema) is actually evaluated here, instead of the
    // gate seeing "no schema" and passing it.
    let p = policy.normalized_declared_view_experimental();
    let tc = evidence.tool_classes.as_deref();
    let approval = applicability(
        nonempty(&p.tools.approval_required),
        nonempty(&p.tools.approval_required_classes),
        tool_name,
        tc,
    );
    let scope = applicability(
        nonempty(&p.tools.restrict_scope),
        nonempty(&p.tools.restrict_scope_classes),
        tool_name,
        tc,
    );
    let redaction = applicability(
        nonempty(&p.tools.redact_args),
        nonempty(&p.tools.redact_args_classes),
        tool_name,
        tc,
    );
    [
        tool_axis(&p, tool_name),
        args_axis(&p, tool_name, args),
        identity_axis(identity_state),
        class_axis(&p, tc),
        obligation_axis(approval, evidence.approval_obtained),
        obligation_axis(scope, evidence.scope_satisfied),
        obligation_axis(redaction, evidence.redaction_applied),
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
/// identity from [`build_record`], and the classification zone carries the [`decision_verdict`] computed
/// with the supplied `evidence`. EXPERIMENTAL. Returns `None` if a digest cannot be canonicalized.
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
    evidence: &DecisionEvidence,
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
        identity_state,
        evidence
    ));
    Some(record)
}

// ── EXPERIMENTAL pack recipe-row binding (cite-by-digest; no pack v2) ─────────────────────────────
// Bind a tool-decision-truth carrier into an Evidence Pack as a proven recipe row: an evidenceRef cites
// the carrier by its content digest, and a coherence_binding ties the row to that citation plus the run
// verdict, so a tampered carrier fails the row closed. Mirrors the private reference-spec; unstable.

/// Recipe id for the pack row (experimental).
pub const RECIPE: &str = "tool_decision_truth.v0";

/// Whether `s` is a well-formed `sha256:<64 lowercase hex>` digest. The pack verifier rejects rows whose
/// citation digests are not this shape, so a malformed or truncated digest cannot pass as a citation.
fn is_sha256_digest(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        None => false,
    }
}

/// Whether `s` is one of the four lattice verdicts. A pack row may only bind a real verdict, not an
/// arbitrary label like `approved`.
fn is_verdict(s: &str) -> bool {
    matches!(s, "match" | "incomplete" | "mismatch" | "invalid")
}

/// Whether `run_verdict` is consistent with the carrier's own `decision_verdict`. A run verdict is the
/// lattice-max over its decisions, so it must be at least as severe as any single decision: a row may not
/// claim `match` while the carrier it cites recorded `mismatch`. A carrier with no (or null) decision
/// verdict imposes no constraint.
fn run_verdict_covers_carrier(carrier: &Value, run_verdict: &str) -> bool {
    match carrier.get("decision_verdict") {
        // Only a genuinely absent verdict imposes no constraint.
        None | Some(Value::Null) => true,
        Some(Value::String(cv)) if is_verdict(cv) => verdict_rank(run_verdict) >= verdict_rank(cv),
        // A present-but-malformed carrier verdict fails closed rather than passing unconstrained.
        _ => false,
    }
}

/// Digest over the decision identity (the two pinned digests). This is the stable logical handle that
/// JOINS a pack row to a carrier; it is NOT the carrier content digest (see [`carrier_content_digest`]).
pub fn decision_identity_digest(
    observed_input_digest: &str,
    declared_policy_digest: &str,
) -> Option<String> {
    let identity = json!({
        "observed_input_digest": observed_input_digest,
        "declared_policy_digest": declared_policy_digest,
    });
    Some(format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(&identity).ok()?)
    ))
}

/// Digest over the FULL canonical carrier record bytes. Unlike [`decision_identity_digest`], this binds
/// every carrier field (observation provenance, classification, labels), so tampering ANY field changes
/// the digest and fails the citing row closed.
pub fn carrier_content_digest(carrier: &Value) -> Option<String> {
    Some(format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(carrier).ok()?)
    ))
}

/// The cross-ecosystem envelope a pack row cites the carrier by. `digest` is the carrier CONTENT digest,
/// and `digest_subject` says so explicitly so a reader never mistakes it for the identity handle.
pub fn evidence_ref(carrier_content_digest: &str, reference: &str) -> Value {
    json!({
        "type": "tool_decision_truth",
        "digest": carrier_content_digest,
        "digest_subject": "carrier_content",
        "canonicalization": "jcs-json-v1",
        "schema": SCHEMA,
        "ref": reference,
    })
}

/// Build a proven recipe row binding a real carrier into the existing Evidence Pack (no pack v2). The row
/// cites the carrier by its CONTENT digest (so tampering any carrier field fails closed) and carries the
/// `decision_identity_digest` as a join key; the `coherence_binding` digests the recipe, the citation, the
/// identity, and the run verdict together. EXPERIMENTAL. `None` if the carrier lacks its identity digests
/// or canonicalization fails.
pub fn pack_recipe_row(carrier: &Value, run_verdict: &str, reference: &str) -> Option<Value> {
    if !is_verdict(run_verdict) || !run_verdict_covers_carrier(carrier, run_verdict) {
        return None;
    }
    let oid = carrier
        .get("observed_input_digest")
        .and_then(|v| v.as_str())?;
    let dpd = carrier
        .get("declared_policy_digest")
        .and_then(|v| v.as_str())?;
    if !is_sha256_digest(oid) || !is_sha256_digest(dpd) {
        return None;
    }
    let content = carrier_content_digest(carrier)?;
    let identity = decision_identity_digest(oid, dpd)?;
    let er = evidence_ref(&content, reference);
    let binding_input = json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "decision_identity_digest": identity,
        "run_verdict": run_verdict,
    });
    let coherence_binding = format!(
        "sha256:{}",
        sha256_hex(&jcs::to_string(&binding_input).ok()?)
    );
    Some(json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "decision_identity_digest": identity,
        "run_verdict": run_verdict,
        "coherence_binding": coherence_binding,
    }))
}

/// Verify a recipe row coheres with the carrier it cites. ALL must hold: the row declares THIS recipe and
/// the tool-decision-truth envelope (type / schema / canonicalization / digest_subject); the citation
/// digest and the join key are well-formed `sha256:` digests; the citation digest equals the recomputed
/// carrier CONTENT digest; the join key equals the identity recomputed from the carrier's OWN embedded
/// digests; the supplied `run_verdict` equals the row's own field; and the `coherence_binding` recomputes
/// from the row's exact fields. Tampering any carrier field, the identity, or the verdict fails closed.
pub fn verify_recipe_row(row: &Value, carrier: &Value, run_verdict: &str) -> bool {
    if row.get("recipe").and_then(|r| r.as_str()) != Some(RECIPE) {
        return false;
    }
    let Some(er) = row.get("evidence_ref").cloned() else {
        return false;
    };
    if er.get("type").and_then(|x| x.as_str()) != Some("tool_decision_truth")
        || er.get("schema").and_then(|x| x.as_str()) != Some(SCHEMA)
        || er.get("canonicalization").and_then(|x| x.as_str()) != Some("jcs-json-v1")
        || er.get("digest_subject").and_then(|x| x.as_str()) != Some("carrier_content")
    {
        return false;
    }
    // A displayed run verdict must not drift from the bound one.
    if row.get("run_verdict").and_then(|v| v.as_str()) != Some(run_verdict) {
        return false;
    }
    // The verdict must be a real lattice verdict and at least as severe as the carrier's own decision, so
    // a row cannot bind a bogus label or claim a cleaner verdict than the carrier it cites.
    if !is_verdict(run_verdict) || !run_verdict_covers_carrier(carrier, run_verdict) {
        return false;
    }
    // Digests must be well-formed before they are trusted as citations.
    let Some(er_digest) = er.get("digest").and_then(|d| d.as_str()) else {
        return false;
    };
    let Some(row_identity) = row.get("decision_identity_digest").and_then(|d| d.as_str()) else {
        return false;
    };
    if !is_sha256_digest(er_digest) || !is_sha256_digest(row_identity) {
        return false;
    }
    // The cited content digest must equal the recomputed carrier content digest.
    match carrier_content_digest(carrier) {
        Some(c) if c == er_digest => {}
        _ => return false,
    }
    // The join key must equal the identity recomputed from the carrier's OWN embedded digests.
    let (Some(oid), Some(dpd)) = (
        carrier
            .get("observed_input_digest")
            .and_then(|v| v.as_str()),
        carrier
            .get("declared_policy_digest")
            .and_then(|v| v.as_str()),
    ) else {
        return false;
    };
    if !is_sha256_digest(oid) || !is_sha256_digest(dpd) {
        return false;
    }
    match decision_identity_digest(oid, dpd) {
        Some(i) if i == row_identity => {}
        _ => return false,
    }
    // Finally the binding must recompute from the row's own fields.
    let binding_input = json!({
        "recipe": RECIPE,
        "evidence_ref": er,
        "decision_identity_digest": row_identity,
        "run_verdict": run_verdict,
    });
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
    // A well-formed declared digest (build_record now rejects malformed ones).
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
        // secret-named keys never affect the digest, at the top level AND nested in objects/arrays.
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
        assert!(mk(DECL).is_some()); // well-formed sha256
        assert!(mk("sha256:decl").is_none()); // too short
        assert!(mk("not-a-digest").is_none()); // wrong prefix
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
        decision_verdict(
            &policy(),
            tool,
            args.as_ref(),
            id,
            &DecisionEvidence::default(),
        )
    }

    fn p_from(v: Value) -> McpPolicy {
        serde_json::from_value(v).unwrap()
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
    fn tool_axis_uses_engine_pattern_semantics() {
        // A literal-prefix deny pattern blocks the matching tool; exact equality would have missed it.
        let p = p_from(json!({
            "version": "1",
            "tools": {"deny": ["delete_*"]},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        let e = DecisionEvidence::default();
        assert_eq!(
            decision_verdict(&p, "delete_all", Some(&json!({})), "present", &e),
            "mismatch"
        );
        assert_eq!(
            decision_verdict(&p, "read_file", Some(&json!({})), "present", &e),
            "match"
        );
    }

    #[test]
    fn declared_constraint_without_evidence_is_incomplete_not_match() {
        // approval_required is declared for the tool but no approval evidence is supplied: the decision
        // can never be `match`; it is at least `incomplete`. With evidence it resolves either way.
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["pay"], "approval_required": ["pay"]},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        assert_eq!(
            decision_verdict(
                &p,
                "pay",
                Some(&json!({})),
                "present",
                &DecisionEvidence::default()
            ),
            "incomplete"
        );
        let approved = DecisionEvidence {
            approval_obtained: Some(true),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "pay", Some(&json!({})), "present", &approved),
            "match"
        );
        let denied = DecisionEvidence {
            approval_obtained: Some(false),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "pay", Some(&json!({})), "present", &denied),
            "mismatch"
        );
    }

    #[test]
    fn class_axis_needs_class_evidence() {
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["x"], "deny_classes": ["network"]},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        // deny_classes declared but no tool-class evidence -> incomplete
        assert_eq!(
            decision_verdict(
                &p,
                "x",
                Some(&json!({})),
                "present",
                &DecisionEvidence::default()
            ),
            "incomplete"
        );
        // tool is in a denied class -> mismatch
        let net = DecisionEvidence {
            tool_classes: Some(vec!["network".into()]),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "x", Some(&json!({})), "present", &net),
            "mismatch"
        );
        // tool not in a denied class -> match
        let fs = DecisionEvidence {
            tool_classes: Some(vec!["fs".into()]),
            ..Default::default()
        };
        assert_eq!(
            decision_verdict(&p, "x", Some(&json!({})), "present", &fs),
            "match"
        );
    }

    #[test]
    fn malformed_declared_schema_is_invalid_not_panic() {
        // A declared schema that does not compile maps to `invalid` (and must not panic the gate).
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["t"]},
            "schemas": {"t": {"$ref": "#/$defs/missing"}},
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        assert_eq!(
            p.check_tool_args_experimental("t", &json!({})),
            ArgsCheck::Malformed
        );
        assert_eq!(
            decision_verdict(
                &p,
                "t",
                Some(&json!({})),
                "present",
                &DecisionEvidence::default()
            ),
            "invalid"
        );
    }

    #[test]
    fn legacy_constraints_are_evaluated_by_the_verdict() {
        // A legacy-constraint-only policy: the declared digest binds a migrated schema, and the verdict
        // must evaluate that SAME migrated schema rather than seeing "no schema" and passing under
        // unconstrained = allow. Before the shared normalized view, the second case was a false `match`.
        let p = p_from(json!({
            "version": "1",
            "tools": {"allow": ["deploy"]},
            "constraints": [{"tool": "deploy", "params": {"env": {"matches": "^prod$"}}}],
            "enforcement": {"unconstrained_tools": "allow"}
        }));
        let e = DecisionEvidence::default();
        assert_eq!(
            decision_verdict(&p, "deploy", Some(&json!({"env": "prod"})), "present", &e),
            "match"
        );
        assert_eq!(
            decision_verdict(
                &p,
                "deploy",
                Some(&json!({"env": "staging"})),
                "present",
                &e
            ),
            "mismatch"
        );
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
        let e = DecisionEvidence::default();
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
            &e,
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
            &e,
        )
        .unwrap();
        assert_eq!(mm["decision_verdict"], json!("mismatch"));
    }
}

#[cfg(test)]
mod pack_tests {
    use super::*;

    const REF: &str = "audit://decision/c1";

    fn pack_policy(allow: &str, deny: &str) -> McpPolicy {
        serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": [allow], "deny": [deny]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap()
    }

    /// A real, fully-classified carrier (real declared digest + decision_verdict): a row cites THIS, not
    /// synthetic digest strings.
    fn carrier() -> Value {
        build_classified_record(
            &pack_policy("deploy", "delete_all"),
            "deploy",
            &json!({"env": "prod"}),
            0,
            b"reference-test-key-v0",
            "test-key-v0",
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap()
    }

    #[test]
    fn evidence_ref_has_the_envelope_fields() {
        let cd = carrier_content_digest(&carrier()).unwrap();
        let er = evidence_ref(&cd, REF);
        let mut keys: Vec<&String> = er.as_object().unwrap().keys().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "canonicalization",
                "digest",
                "digest_subject",
                "ref",
                "schema",
                "type"
            ]
        );
        assert_eq!(er["schema"], json!(SCHEMA));
        assert_eq!(er["digest"], json!(cd));
        assert_eq!(er["digest_subject"], json!("carrier_content"));
    }

    #[test]
    fn content_and_identity_digests_are_distinct_and_well_formed() {
        let c = carrier();
        let content = carrier_content_digest(&c).unwrap();
        let identity = decision_identity_digest(
            c["observed_input_digest"].as_str().unwrap(),
            c["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        // Different subjects -> different digests, both well-formed sha256.
        assert_ne!(content, identity);
        assert!(super::is_sha256_digest(&content));
        assert!(super::is_sha256_digest(&identity));
    }

    #[test]
    fn recipe_row_coheres_with_its_real_carrier() {
        let c = carrier();
        let row = pack_recipe_row(&c, "match", REF).unwrap();
        assert!(verify_recipe_row(&row, &c, "match"));
        // The row cites the carrier CONTENT digest (so any field is bound), and carries the identity join.
        assert_eq!(
            row["evidence_ref"]["digest"],
            json!(carrier_content_digest(&c).unwrap())
        );
        assert!(row.get("decision_identity_digest").is_some());
    }

    #[test]
    fn tampering_any_carrier_field_or_the_verdict_fails_closed() {
        let c = carrier();
        let row = pack_recipe_row(&c, "match", REF).unwrap();
        // Tampering a NON-identity provenance field still fails closed (content digest moved).
        let mut tampered = c.clone();
        tampered["result_status"] = json!("error");
        assert!(!verify_recipe_row(&row, &tampered, "match"));
        // Tampering the embedded identity fails closed too.
        let mut tampered_id = c.clone();
        tampered_id["observed_input_digest"] = json!("sha256:deadbeef");
        assert!(!verify_recipe_row(&row, &tampered_id, "match"));
        // A changed run verdict (vs the row's own field) fails closed.
        assert!(!verify_recipe_row(&row, &c, "mismatch"));
    }

    /// A row whose coherence_binding is self-consistent over the given (possibly foreign) recipe/envelope.
    fn coherent_row(recipe: &str, er: Value, identity: &str, run_verdict: &str) -> Value {
        let binding_input = json!({
            "recipe": recipe,
            "evidence_ref": er,
            "decision_identity_digest": identity,
            "run_verdict": run_verdict,
        });
        let cb = format!(
            "sha256:{}",
            crate::fingerprint::sha256_hex(&crate::mcp::jcs::to_string(&binding_input).unwrap())
        );
        json!({
            "recipe": recipe,
            "evidence_ref": er,
            "decision_identity_digest": identity,
            "run_verdict": run_verdict,
            "coherence_binding": cb,
        })
    }

    #[test]
    fn verify_rejects_foreign_recipe_or_envelope_even_when_self_coherent() {
        let c = carrier();
        let content = carrier_content_digest(&c).unwrap();
        let identity = decision_identity_digest(
            c["observed_input_digest"].as_str().unwrap(),
            c["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let good_er = evidence_ref(&content, REF);
        // Sanity: the proper recipe + envelope verifies against the real carrier.
        assert!(verify_recipe_row(
            &coherent_row(RECIPE, good_er.clone(), &identity, "match"),
            &c,
            "match"
        ));
        // A self-coherent row with a FOREIGN recipe is rejected.
        assert!(!verify_recipe_row(
            &coherent_row("other.recipe.v0", good_er.clone(), &identity, "match"),
            &c,
            "match"
        ));
        // Foreign envelope type / schema / canonicalization / digest_subject are each rejected.
        for (field, bad) in [
            ("type", json!("other")),
            ("schema", json!("other.schema.v0")),
            ("canonicalization", json!("cbor-deterministic-v1")),
            ("digest_subject", json!("decision_identity")),
        ] {
            let mut er = good_er.as_object().unwrap().clone();
            er.insert(field.to_string(), bad);
            let row = coherent_row(RECIPE, Value::Object(er), &identity, "match");
            assert!(
                !verify_recipe_row(&row, &c, "match"),
                "envelope field {field} must be rejected"
            );
        }
    }

    #[test]
    fn verify_rejects_malformed_citation_digests() {
        let c = carrier();
        let identity = decision_identity_digest(
            c["observed_input_digest"].as_str().unwrap(),
            c["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        // A citation digest that is not sha256:<64 hex> is rejected even if everything else is coherent.
        let bad_er = evidence_ref("sha256:short", REF);
        assert!(!verify_recipe_row(
            &coherent_row(RECIPE, bad_er, &identity, "match"),
            &c,
            "match"
        ));
        // A malformed identity join key is rejected too.
        let good_er = evidence_ref(&carrier_content_digest(&c).unwrap(), REF);
        assert!(!verify_recipe_row(
            &coherent_row(RECIPE, good_er, "not-a-digest", "match"),
            &c,
            "match"
        ));
    }

    #[test]
    fn verify_rejects_carrier_with_malformed_embedded_digest() {
        // Even if a row cites the carrier's content + identity perfectly, a malformed embedded digest in
        // the carrier is rejected: identity recomputation must not trust an ill-formed input.
        let mut bad = carrier();
        bad["observed_input_digest"] = json!("sha256:bad"); // not 64 hex
        let content = carrier_content_digest(&bad).unwrap();
        let identity = decision_identity_digest(
            "sha256:bad",
            bad["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let row = coherent_row(RECIPE, evidence_ref(&content, REF), &identity, "match");
        assert!(!verify_recipe_row(&row, &bad, "match"));
    }

    #[test]
    fn pack_row_rejects_bogus_or_understated_verdict() {
        let c = carrier();
        assert_eq!(c["decision_verdict"], json!("match"));
        // A non-verdict label is refused outright.
        assert!(pack_recipe_row(&c, "approved", REF).is_none());

        // A carrier whose own decision is `mismatch` cannot be cited with a cleaner run verdict.
        let mismatch_carrier = build_classified_record(
            &pack_policy("deploy", "delete_all"),
            "delete_all",
            &json!({}),
            0,
            b"reference-test-key-v0",
            "test-key-v0",
            "authoritative_boundary",
            "c1",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .unwrap();
        assert_eq!(mismatch_carrier["decision_verdict"], json!("mismatch"));
        assert!(pack_recipe_row(&mismatch_carrier, "match", REF).is_none()); // understated -> refused
        let row = pack_recipe_row(&mismatch_carrier, "mismatch", REF).unwrap(); // >= carrier verdict
        assert!(verify_recipe_row(&row, &mismatch_carrier, "mismatch"));

        // verify also rejects a hand-crafted row that understates the carrier verdict.
        let content = carrier_content_digest(&mismatch_carrier).unwrap();
        let identity = decision_identity_digest(
            mismatch_carrier["observed_input_digest"].as_str().unwrap(),
            mismatch_carrier["declared_policy_digest"].as_str().unwrap(),
        )
        .unwrap();
        let understating = coherent_row(RECIPE, evidence_ref(&content, REF), &identity, "match");
        assert!(!verify_recipe_row(
            &understating,
            &mismatch_carrier,
            "match"
        ));

        // A carrier carrying a malformed (non-lattice) decision_verdict fails closed: it imposes a
        // constraint no run verdict can satisfy, so no row may cite it.
        let mut malformed = carrier();
        malformed["decision_verdict"] = json!("approved");
        assert!(pack_recipe_row(&malformed, "match", REF).is_none());
        assert!(pack_recipe_row(&malformed, "invalid", REF).is_none());
    }
}
