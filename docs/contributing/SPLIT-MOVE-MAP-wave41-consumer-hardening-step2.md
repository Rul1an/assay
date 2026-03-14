# SPLIT MOVE MAP - Wave41 Consumer Hardening Step2

## Intent
Keep Wave41 Step2 bounded to additive consumer/read precedence metadata over the existing decision and replay compatibility surfaces.

## Allowed implementation scope
- `crates/assay-core/src/mcp/decision.rs`
- `crates/assay-core/src/mcp/decision/consumer_contract.rs`
- `crates/assay-core/src/mcp/decision/replay_diff.rs`
- `crates/assay-core/tests/decision_emit_invariant.rs`
- `crates/assay-core/tests/replay_diff_contract.rs`
- Step2 docs/gate files

## Move rationale
- `decision.rs`: additive event payload fields and normalization hook-up
- `consumer_contract.rs`: bounded consumer-read precedence projection
- `replay_diff.rs`: replay basis projection and fallback reconstruction
- tests: consumer-facing invariants and fallback coverage

## Explicitly out of scope
- tool-call handler changes
- policy evaluation changes
- CLI/runtime consumer changes outside replay/decision payload tests
- MCP server changes
- workflow changes
