# SPLIT CHECKLIST - Wave 51 MCP Proxy Step3

## Scope Lock

- Characterize `crates/assay-core/src/mcp/proxy.rs` before any module split.
- Preserve `McpProxy` as the stable facade for spawn/run behavior.
- Add focused contract tests around JSON-RPC tool-call idempotency, policy reason-code mapping, tool-definition observation, and decision-event field projection.
- Do not move proxy code yet.
- Do not change policy semantics, stdio passthrough behavior, audit logging, decision emission, workflows, or generated files.

## Files

- `crates/assay-core/src/mcp/proxy.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-mcp-proxy-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-mcp-proxy-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-mcp-proxy-step3.md`
- `scripts/ci/review-wave51-hotspot-mcp-proxy-step3.sh`

## Drift Gates

- `McpProxy` remains in `mcp/proxy.rs`.
- `McpProxy::run` remains in `mcp/proxy.rs`.
- server-to-client and client-to-server thread boundaries remain visible in `mcp/proxy.rs`.
- deny responses still use `make_deny_response`.
- decision event emission still flows through `emit_decision`.
- tool-definition enrichment still flows through `observe_tool_definition`.
- idempotency keys still flow through `extract_tool_call_id`.
- no `crates/assay-core/src/mcp/proxy/` submodule directory is introduced in Step 3.

## Validation

```bash
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
bash scripts/ci/review-wave51-hotspot-mcp-proxy-step3.sh
```

## Definition of Done

- Step 3 reviewer script passes.
- `proxy_contract_*` tests pass.
- No proxy code is moved yet.
- Step 4 can split proxy internals using these tests as a behavioral safety net.
