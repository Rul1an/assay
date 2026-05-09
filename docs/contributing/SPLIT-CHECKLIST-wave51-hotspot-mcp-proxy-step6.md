# SPLIT CHECKLIST - Wave 51 MCP Proxy Step6

## Scope Lock

- Move only the client-to-server stdin/policy/forwarding loop behind the existing `McpProxy` facade.
- Keep `McpProxy::spawn` and `McpProxy::run` in `crates/assay-core/src/mcp/proxy.rs`.
- Keep thread spawning, thread joining, child lifecycle, and facade state in `proxy.rs`.
- Preserve stdin line reading, JSON-RPC parsing, identity/tool-definition cache reads, policy evaluation, allow/warning/deny handling, dry-run forwarding, deny response writing, suspicious JSON warning, and child stdin forwarding.
- Do not change policy semantics, JSON-RPC surfaces, audit payloads, decision-event payloads, server-to-client behavior, workflows, or generated files.

## Files

- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/src/mcp/proxy/client.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-mcp-proxy-step6.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-mcp-proxy-step6.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-mcp-proxy-step6.md`
- `scripts/ci/review-wave51-hotspot-mcp-proxy-step6.sh`

## Drift Gates

- `proxy.rs` non-test code stays under 260 lines.
- `proxy.rs` declares `mod client;` and calls `run_client_to_server`.
- `proxy.rs` retains `pub struct McpProxy`, `pub fn spawn`, `pub fn run`, `thread::spawn`, and child `wait`.
- `proxy.rs` no longer directly imports/uses `AuditLog`, `PolicyDecision`, `PolicyState`, `JsonRpcRequest`, `make_deny_response`, `stdin.lock`, `read_line`, `write_all`, or direct decision helpers.
- `proxy/client.rs` owns `run_client_to_server`, `stdin.lock`, `read_line`, `PolicyDecision`, `PolicyState`, `AuditLog`, `make_deny_response`, `emit_decision`, `handle_allow`, `map_policy_code`, and child stdin forwarding.
- `proxy/client.rs` must not call `observe_tool_definition`; server output enrichment remains in `proxy/server.rs`.

## Validation

```bash
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
bash scripts/ci/review-wave51-hotspot-mcp-proxy-step6.sh
```

## Definition of Done

- Step 6 reviewer script passes.
- Step 3/4/5 proxy contract tests pass after the client-loop move.
- `proxy.rs` remains the stable public facade and owns only orchestration, not per-line policy forwarding.
- Step 7 can decide whether to stop Wave 51 MCP proxy here or further split client policy branches into smaller units.
