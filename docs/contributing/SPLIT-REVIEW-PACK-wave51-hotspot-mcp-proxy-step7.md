# SPLIT REVIEW PACK - Wave 51 MCP Proxy Step7

## Summary

Step 7 extracts MCP proxy client policy-decision branch handling from `proxy/client.rs` into private module `proxy/client/branches.rs`. This keeps the client loop focused on read/parse/cache/evaluate/forward orchestration and gives allow, warning, deny, and dry-run deny their own branch-level contracts.

## LOC Delta

| File | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `crates/assay-core/src/mcp/proxy/client.rs` | 226 | 137 | -89 |
| `crates/assay-core/src/mcp/proxy/client/branches.rs` | 0 | 356 | +356 |

## Boundary Proof

Client loop remains the loop/coordinator:

```bash
rg -n 'fn run_client_to_server|stdin\.lock|read_line|serde_json::from_str::<JsonRpcRequest>|handle_policy_decision|child_stdin\.write_all' crates/assay-core/src/mcp/proxy/client.rs
```

Branch module owns policy branches:

```bash
rg -n 'fn handle_policy_decision|PolicyDecision::Allow|PolicyDecision::AllowWithWarning|PolicyDecision::Deny|make_deny_response|BranchOutcome::Blocked' crates/assay-core/src/mcp/proxy/client/branches.rs
```

Client loop no longer owns policy branch mechanics:

```bash
! rg -n 'PolicyDecision::Allow|PolicyDecision::AllowWithWarning|PolicyDecision::Deny|make_deny_response|map_policy_code|AuditEvent' crates/assay-core/src/mcp/proxy/client.rs
```

## Validation

- `cargo fmt --check`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib proxy_contract_client_branch_`
- `cargo test -p assay-core --lib proxy_contract_`
- `cargo test -p assay-core --lib mcp::proxy::`
- `bash scripts/ci/review-wave51-hotspot-mcp-proxy-step7.sh`

## Next Step

After Step 7 lands, stop the MCP proxy split unless a concrete reviewer concern or new hotspot analysis shows `proxy/client/branches.rs` needs a second internal split. The facade and loop boundaries are now small enough for routine review.
