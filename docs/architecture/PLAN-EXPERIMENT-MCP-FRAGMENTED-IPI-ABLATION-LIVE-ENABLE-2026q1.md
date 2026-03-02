# PLAN - Ablation Live Enable: MCP Fragmented IPI Mitigation (2026Q1)

## Intent
Enable truthful live execution for the fragmented IPI ablation harness so causal attribution can be measured against a real MCP host path rather than only the current local mock harness.

This is a docs-only freeze slice (StepA). No runtime or workflow changes.

## Context (current state)
- The current ablation harness supports local mock execution only.
- `RUN_LIVE=1` is not yet supported in `scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh`.
- Mock ablation results are useful for wiring and contract verification, but they are not sufficient for live mechanism attribution.

## Live-enable scope (frozen)
The next implementation slice must add `RUN_LIVE=1` support to:
- `scripts/ci/test-exp-mcp-fragmented-ipi-ablation.sh`
- `scripts/ci/exp-mcp-fragmented-ipi/ablation/run_variant.sh`
- the existing baseline/protected ablation runners they invoke

Live mode must remain opt-in:
- `RUN_LIVE=0` stays the CI-safe default
- `RUN_LIVE=1` is local/manual only

## Required environment contract (frozen)
For `RUN_LIVE=1`:
- `MCP_HOST_CMD` is required
- `MCP_HOST_ARGS` is optional
- `ASSAY_CMD` defaults to `assay` if not set explicitly

For `RUN_LIVE=0`:
- no live MCP host is required
- the current mock/offline path remains valid and required in CI

## Mode semantics (unchanged)
Live-enable must preserve the current ablation semantics exactly:
- `wrap_only`: sequence sidecar disabled
- `sequence_only`: sequence sidecar enabled and wrap policy kept permissive for exfil-blocking rules
- `combined`: sequence sidecar enabled and blocking wrap policy enabled

## Logging invariants (frozen)
Every protected live run must log the following markers explicitly:
- `ABLATION_MODE=<mode>`
- `SIDECAR=enabled|disabled`
- `ASSAY_POLICY=<path-or-name>`
- `MCP_HOST_CMD=<command>`

These markers exist for auditability and post-run mechanism attribution.

## Safety constraints
The live-enable slice must preserve the existing safety boundaries:
- no absolute user-specific paths hardcoded in repo scripts or docs
- no workflow changes
- no auto-running live mode in CI
- no prompt/body logging of sensitive document contents in repo artifacts
- only canary signal, rule id, and minimal redacted argument summaries should be logged for security evidence

## Non-goals
- No new sink classes beyond the current `web_search` sink
- No taint/label propagation claims
- No entropy enforcement
- No changes to experiment scoring semantics
- No policy auto-synthesis or semantic clustering claims

## Acceptance criteria (StepA)
- `RUN_LIVE=1` support target files are explicitly frozen
- required environment variables are frozen
- mode semantics remain unchanged
- logging invariants are explicit and auditable
- docs explicitly state that live mode is local/manual, not CI-enforced
- no runtime/workflow changes in this slice
