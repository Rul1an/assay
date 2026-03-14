# SPLIT MOVE MAP - Wave40 Deny Evidence Step2

## Intent
Bounded implementation for deny/fail-closed/enforcement evidence convergence with deterministic precedence and additive legacy fallback, without runtime behavior change.

## Touched runtime paths
- `crates/assay-core/src/mcp/decision/deny_convergence.rs`
  - adds deterministic deny-classification projector and precedence chain
  - defines additive deny classification source and precedence version
- `crates/assay-core/src/mcp/decision.rs`
  - adds additive Decision Event deny-convergence fields
  - populates fields from projector during normalization flow
- `crates/assay-core/src/mcp/decision/replay_diff.rs`
  - extends replay basis with deny-convergence compatibility fields
  - derives deterministic fallback metadata from decision payloads

## Touched tests
- `crates/assay-core/tests/replay_diff_contract.rs`
  - validates additive deny-convergence basis fields
  - validates deny fallback precedence behavior for missing legacy shape markers
- `crates/assay-core/tests/decision_emit_invariant.rs`
  - validates emitted events include deterministic deny-convergence markers

## Out-of-scope guarantees
- no runtime behavior change
- no new deny semantics or obligation types
- no policy backend/control-plane/auth transport scope expansion
- no workflow changes
