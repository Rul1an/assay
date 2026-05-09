# SPLIT REVIEW PACK - Wave 51 MCP Proxy Step4

## Summary

Step 4 performs the first MCP proxy module split by moving pure helper responsibilities into private `proxy/*` modules. `McpProxy::run` remains the stable facade for child process lifecycle and threaded stdio passthrough.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-core/src/mcp/proxy.rs` | 979 | 540 | -439 |
| `crates/assay-core/src/mcp/proxy/decisions.rs` | 0 | 364 | +364 |
| `crates/assay-core/src/mcp/proxy/tools.rs` | 0 | 84 | +84 |

## Boundary Proof

Facade stays in place:

```bash
rg -n 'pub struct McpProxy|pub fn spawn\(|pub fn run\(|thread::spawn|make_deny_response' crates/assay-core/src/mcp/proxy.rs
```

Moved decision helpers:

```bash
rg -n 'fn handle_allow|fn extract_tool_call_id|fn map_policy_code|fn emit_decision' crates/assay-core/src/mcp/proxy/decisions.rs
```

Moved tools/list helper:

```bash
rg -n 'struct ToolDefinitionObservation|fn observe_tool_definition' crates/assay-core/src/mcp/proxy/tools.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib proxy_contract_`
- `cargo test -p assay-core --lib mcp::proxy::`
- `bash scripts/ci/review-wave51-hotspot-mcp-proxy-step4.sh`

## Next Step

Step 5 should move one threaded loop at a time. The lower-risk next candidate is the server-to-client tools/list enrichment loop, because Step 4 already isolates tools/list observation and keeps the forwarding contract visible.
