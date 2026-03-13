# SPLIT MOVE MAP - Wave34 Fail-Closed Step2

## Intent
Bounded implementation for fail-closed matrix typing and additive decision evidence.

## Touched runtime paths
- `crates/assay-core/src/mcp/policy/mod.rs`
  - adds fail-closed typed surface (`ToolRiskClass`, `FailClosedMode`, `FailClosedTrigger`, `FailClosedContext`)
- `crates/assay-core/src/mcp/decision.rs`
  - adds additive `fail_closed` context into decision payload + policy context plumbing
- `crates/assay-core/src/mcp/tool_call_handler/emit.rs`
  - passes fail-closed context from policy metadata to decision context
- `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`
  - seeds deterministic fail-closed defaults and marks bounded context/runtime dependency failures
- `crates/assay-core/src/mcp/proxy.rs`
  - mirrors additive fail-closed context in proxy-emitted decision events

## Touched tests
- `crates/assay-core/src/mcp/tool_call_handler/tests.rs`
  - adds assertions for additive fail-closed context defaults and context-provider failure mapping

## Out of scope guarantees
- no new obligations or policy language changes
- no auth transport changes
- no control-plane expansion
- no external incident/case integrations
