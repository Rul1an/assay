# SPLIT REVIEW PACK - Wave 51 MCP Proxy Step3

## Summary

Step 3 prepares the MCP proxy hotspot for a later module split by adding contract tests and a review gate. No proxy implementation blocks are moved in this step.

## Behavioral Contracts Added

- explicit MCP `_meta.tool_call_id` remains the preferred idempotency key.
- string and numeric JSON-RPC request ids remain deterministic fallback keys.
- missing request ids still generate `gen_*` fallback keys.
- legacy policy error codes still map to stable decision-event reason codes.
- unknown policy error codes still normalize to `P_POLICY_DENY`.
- empty tool names are not observed and do not gain `tool_identity`.
- decision event projection preserves the core request, policy, match, and auth-context fields used downstream.

## Boundary Proof

```bash
rg -n 'pub struct McpProxy|pub fn run\(|thread::spawn|make_deny_response|emit_decision|observe_tool_definition|extract_tool_call_id' crates/assay-core/src/mcp/proxy.rs
find crates/assay-core/src/mcp -maxdepth 2 -type f | sort | rg 'mcp/proxy/'
```

The second command should return no files for Step 3.

## Validation

- `cargo fmt --check`
- `cargo check -p assay-core`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo test -p assay-core --lib proxy_contract_`
- `bash scripts/ci/review-wave51-hotspot-mcp-proxy-step3.sh`

## Next Step

Step 4 can perform the actual proxy split with the Step 3 tests acting as the no-regression harness. Recommended first split target: tool-call idempotency, policy-code mapping, and decision emission helpers, because those are pure and easiest to verify before moving the threaded stdio loops.
