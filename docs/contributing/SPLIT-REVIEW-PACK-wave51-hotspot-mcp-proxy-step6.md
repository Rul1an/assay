# SPLIT REVIEW PACK - Wave 51 MCP Proxy Step6

## Summary

Step 6 moves the MCP proxy client-to-server stdin/policy/forwarding loop into `proxy/client.rs`. The stable facade still owns `McpProxy::spawn`, `McpProxy::run`, thread spawning/joining, child lifecycle, decision-emitter initialization, event-source derivation, and cache wiring.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-core/src/mcp/proxy.rs` | 506 | 331 | -175 |
| `crates/assay-core/src/mcp/proxy/client.rs` | 0 | 219 | +219 |

## Boundary Proof

Facade still owns orchestration:

```bash
rg -n 'pub struct McpProxy|pub fn spawn\(|pub fn run\(|thread::spawn|run_client_to_server|run_server_to_client|self\.child\.wait' crates/assay-core/src/mcp/proxy.rs
```

Client loop moved:

```bash
rg -n 'fn run_client_to_server|stdin\.lock|read_line|PolicyDecision|PolicyState|AuditLog|make_deny_response|emit_decision|child_stdin\.write_all' crates/assay-core/src/mcp/proxy/client.rs
```

Facade no longer owns policy forwarding directly:

```bash
! rg -n 'stdin\.lock|PolicyDecision|PolicyState|AuditLog|JsonRpcRequest|make_deny_response|child_stdin\.write_all|emit_decision|handle_allow|map_policy_code|extract_tool_call_id' crates/assay-core/src/mcp/proxy.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib proxy_contract_`
- `cargo test -p assay-core --lib mcp::proxy::`
- `bash scripts/ci/review-wave51-hotspot-mcp-proxy-step6.sh`

## Next Step

Step 7 should either stop the MCP proxy split at a maintainable facade or split `proxy/client.rs` internally by policy branch after adding branch-level behavior contracts for allow, warning, deny, and dry-run deny.
