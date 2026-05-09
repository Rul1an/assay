# SPLIT CHECKLIST - Wave 51 MCP Proxy Step7

## Scope Lock

- Split only the MCP proxy client policy-decision branch handling out of `proxy/client.rs`.
- Keep `proxy/client.rs::run_client_to_server` as the stdin/read/parse/cache/evaluate/forward loop.
- Move allow, allow-with-warning, deny, and dry-run-deny response decisions into `proxy/client/branches.rs`.
- Add branch-level behavior contracts for allow, warning, deny, and dry-run deny before relying on the split.
- Do not change policy semantics, JSON-RPC surfaces, audit payloads, decision-event payloads, server-to-client behavior, workflows, or generated files.

## Files

- `crates/assay-core/src/mcp/proxy/client.rs`
- `crates/assay-core/src/mcp/proxy/client/branches.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-mcp-proxy-step7.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-mcp-proxy-step7.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-mcp-proxy-step7.md`
- `scripts/ci/review-wave51-hotspot-mcp-proxy-step7.sh`

## Drift Gates

- `proxy.rs` remains the stable public facade and keeps orchestration only.
- `proxy/client.rs` owns stdin locking, line reads, JSON-RPC parsing, identity/tool-definition cache lookup, policy evaluation, suspicious JSON warning, and child stdin forwarding.
- `proxy/client.rs` delegates policy branch handling through `handle_policy_decision` and `PolicyBranchContext`.
- `proxy/client.rs` must not directly match `PolicyDecision::{Allow, AllowWithWarning, Deny}` or own deny-response construction.
- `proxy/client/branches.rs` owns `PolicyDecision` branch matching, allow/warning/deny audit decisions, decision emission, reason-code mapping, deny response generation, and branch outcomes.
- `proxy/client/branches.rs` must not own stdin reading, child stdin forwarding, server-output enrichment, or tools/list observation.

## Validation

```bash
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_client_branch_
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
bash scripts/ci/review-wave51-hotspot-mcp-proxy-step7.sh
```

## Definition of Done

- Step 7 reviewer script passes.
- Four branch-level contracts pass: allow, warning, deny, and dry-run deny.
- `proxy/client.rs` is a loop/coordinator, not a policy branch implementation file.
- `proxy/client/branches.rs` is private to the client module and does not broaden public API surface.
