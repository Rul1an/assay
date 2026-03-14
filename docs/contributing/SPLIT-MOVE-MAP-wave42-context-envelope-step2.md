# SPLIT MOVE MAP - Wave42 Context Envelope Step2

## Intent
Keep Wave42 Step2 bounded to additive context-envelope completeness metadata over the existing decision payload surfaces.

## Allowed implementation scope
- `crates/assay-core/src/mcp/decision.rs`
- `crates/assay-core/src/mcp/decision/context_contract.rs`
- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-core/tests/decision_emit_invariant.rs`
- Step2 docs/gate files

## Move rationale
- `decision.rs`: additive event payload fields and normalization hook-up
- `context_contract.rs`: bounded context-envelope completeness projection
- `proxy.rs`: align proxy-emitted decision payloads with the same additive contract projection
- tests: downstream context-envelope invariants and completeness coverage

## Explicitly out of scope
- tool-call handler behavior changes
- policy evaluation changes
- replay classification changes
- CLI/runtime consumer changes outside existing decision payload tests
- MCP server changes
- workflow changes
