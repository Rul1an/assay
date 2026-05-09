# SPLIT CHECKLIST - Wave 51 MCP Proxy Step5

## Scope Lock

- Move only the server-to-client child stdout loop behind the existing `McpProxy` facade.
- Keep `McpProxy::spawn` and `McpProxy::run` in `crates/assay-core/src/mcp/proxy.rs`.
- Keep the client-to-server policy loop in `proxy.rs` for this step.
- Preserve stdout passthrough, per-line processing, tools/list enrichment, identity cache updates, and tool-definition cache updates.
- Do not change policy semantics, JSON-RPC parsing, deny responses, audit logging, decision-event payloads, workflows, or generated files.

## Files

- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/src/mcp/proxy/server.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-mcp-proxy-step5.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-mcp-proxy-step5.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-mcp-proxy-step5.md`
- `scripts/ci/review-wave51-hotspot-mcp-proxy-step5.sh`

## Drift Gates

- `proxy.rs` non-test code stays under 420 lines.
- `proxy.rs` declares `mod server;` and calls `run_server_to_client`.
- `proxy.rs` retains `pub struct McpProxy`, `pub fn spawn`, `pub fn run`, `thread::spawn`, `make_deny_response`, and the client-to-server `stdin.lock()` loop.
- `proxy.rs` no longer imports or uses `BufReader`.
- `proxy.rs` no longer directly calls `observe_tool_definition`.
- `proxy/server.rs` owns `run_server_to_client`, `BufReader`, `read_line`, `observe_tool_definition`, identity cache insertion, and tool-definition cache insertion.

## Validation

```bash
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
bash scripts/ci/review-wave51-hotspot-mcp-proxy-step5.sh
```

## Definition of Done

- Step 5 reviewer script passes.
- Step 3/4 proxy contract tests pass after the server-loop move.
- `proxy.rs` remains the stable public facade and still owns the client-to-server policy loop.
- Step 6 can move the client-to-server policy loop once deny/allow forwarding seams are characterized enough.
