# SPLIT CHECKLIST - Wave 51 MCP Proxy Step4

## Scope Lock

- Split pure MCP proxy helpers behind the existing `McpProxy` facade.
- Keep `McpProxy::spawn` and `McpProxy::run` in `crates/assay-core/src/mcp/proxy.rs`.
- Keep threaded stdio passthrough and child lifecycle in `proxy.rs` for this step.
- Move decision/idempotency/audit-allow helpers into `proxy/decisions.rs`.
- Move tools/list observation and tool-definition binding into `proxy/tools.rs`.
- Preserve Step 3 `proxy_contract_*` behavior.
- Do not change policy semantics, JSON-RPC surfaces, event payload shape, workflows, or generated files.

## Files

- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/src/mcp/proxy/decisions.rs`
- `crates/assay-core/src/mcp/proxy/tools.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-mcp-proxy-step4.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-mcp-proxy-step4.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-mcp-proxy-step4.md`
- `scripts/ci/review-wave51-hotspot-mcp-proxy-step4.sh`

## Drift Gates

- `proxy.rs` non-test code stays under 450 lines.
- `proxy.rs` declares `mod decisions;` and `mod tools;`.
- `proxy.rs` retains `pub struct McpProxy`, `pub fn spawn`, `pub fn run`, `thread::spawn`, and `make_deny_response`.
- `proxy.rs` no longer owns helper definitions for `extract_tool_call_id`, `map_policy_code`, `emit_decision`, or `observe_tool_definition`.
- `proxy/decisions.rs` owns `handle_allow`, `extract_tool_call_id`, `map_policy_code`, and `emit_decision`.
- `proxy/tools.rs` owns `observe_tool_definition` and `ToolDefinitionObservation`.

## Validation

```bash
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib proxy_contract_
cargo test -p assay-core --lib mcp::proxy::
bash scripts/ci/review-wave51-hotspot-mcp-proxy-step4.sh
```

## Definition of Done

- Step 4 reviewer script passes.
- Step 3 contract tests pass from their new module homes.
- `proxy.rs` remains the stable facade for process/thread behavior.
- Step 5 can move the server-to-client tools/list loop or client-to-server policy loop behind these module seams.
