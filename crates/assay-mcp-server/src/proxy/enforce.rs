//! P61e-c1/c2/c3: the enforcing-proxy policy decision point — caller-allowance + credential-scope +
//! drift gates, plus the first allow/forward path. Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! Scope: parse the `--enforce-policy` file and the `--declared-mcp-manifest` baseline, classify an
//! observed `tools/call`, and decide. c1 added the caller-allowance gate; c2 the credential-scope gate;
//! c3 the drift gate (the current observed per-tool digest must equal the approved baseline digest,
//! with both a baseline and a current COMPLETE observation required) AND the first allow path: a call
//! that clears every gate is forwarded. The temporary `pdp_gate_unavailable` reason is gone — every
//! outcome is now either a precedence-pinned deny or an allow.
//!
//! Caller identity is the static `caller.id` from the policy only — no transport/env/request
//! inference — so a single stdio session is bound to one configured caller and `unknown_caller`
//! cannot occur at runtime (a policy without `caller.id` fails startup).

mod allowance;
mod credential_scope;
mod decision;
mod manifest;
mod policy;
mod records;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use allowance::target_digest;
#[allow(unused_imports)]
pub use decision::{decide, Decision};
#[allow(unused_imports)]
pub use manifest::{load_declared_manifest, BaselineTool, DeclaredManifest, ObservedToolDigest};
#[allow(unused_imports)]
pub use policy::{
    load, Allowance, Caller, EnforceInputs, EnforcePolicy, Target, UpstreamCredential,
};
pub use records::decision_record;
