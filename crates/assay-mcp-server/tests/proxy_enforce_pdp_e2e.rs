//! P61e-c3: the enforcing-proxy PDP end to end — caller-allowance + credential-scope + drift gates,
//! and the first allow/forward path. Spec: docs/reference/mcp-upstream-proxy-enforcement.md.
//!
//! Deny-first: the deny matrix is asserted first, and across all of it no `tools/call` ever reaches the
//! upstream. The ONE happy path (full policy + approved baseline + a current complete observation whose
//! per-tool digest matches) is the only case that forwards. `--declared-mcp-manifest` is required in
//! enforcing mode; a missing/invalid policy OR baseline fails startup, never a runtime deny.

#[path = "proxy_enforce_pdp_e2e/conformance.rs"]
mod conformance;
#[path = "proxy_enforce_pdp_e2e/drift_allow.rs"]
mod drift_allow;
#[path = "proxy_enforce_pdp_e2e/establish.rs"]
mod establish;
#[path = "proxy_enforce_pdp_e2e/pre_drift.rs"]
mod pre_drift;
#[path = "proxy_enforce_pdp_e2e/startup.rs"]
mod startup;
#[path = "proxy_enforce_pdp_e2e/support.rs"]
mod support;
