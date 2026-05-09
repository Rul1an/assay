# SPLIT REVIEW PACK - Wave 51 MCP Proxy Step5

## Summary

Step 5 moves the MCP proxy server-to-client child stdout loop into `proxy/server.rs`. The stable facade still owns `McpProxy::spawn`, `McpProxy::run`, thread spawning, child lifecycle, deny responses, and the client-to-server policy loop.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-core/src/mcp/proxy.rs` | 540 | 506 | -34 |
| `crates/assay-core/src/mcp/proxy/server.rs` | 0 | 65 | +65 |

## Boundary Proof

Facade still owns process/thread shape:

```bash
rg -n 'pub struct McpProxy|pub fn spawn\(|pub fn run\(|thread::spawn|make_deny_response|stdin\.lock' crates/assay-core/src/mcp/proxy.rs
```

Server loop moved:

```bash
rg -n 'fn run_server_to_client|BufReader|read_line|observe_tool_definition|identity_cache|tool_definition_cache' crates/assay-core/src/mcp/proxy/server.rs
```

Facade no longer parses server output directly:

```bash
! rg -n 'BufReader|observe_tool_definition' crates/assay-core/src/mcp/proxy.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib proxy_contract_`
- `cargo test -p assay-core --lib mcp::proxy::`
- `bash scripts/ci/review-wave51-hotspot-mcp-proxy-step5.sh`

## Next Step

Step 6 should characterize and then move the client-to-server policy loop. That split is riskier because it owns forwarding, deny responses, dry-run behavior, audit logging, and decision emission.
