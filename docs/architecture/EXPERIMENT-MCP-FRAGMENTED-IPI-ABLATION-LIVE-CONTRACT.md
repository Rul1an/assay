# Live Execution Contract - MCP Fragmented IPI Ablation (StepA Freeze)

## Intent
Freeze the behavioral and configuration contract for adding live execution support to the fragmented IPI ablation harness.

This contract is tied to current codebase reality:
- sequence/state enforcement exists
- live ablation is not yet wired
- the goal is truthful mechanism attribution, not broader feature expansion

This document freezes behavior, not exact shell syntax.

## Live mode switch (frozen)
- `RUN_LIVE=0`: offline/mock execution path
- `RUN_LIVE=1`: live execution path using a real MCP host command

The harness must reject `RUN_LIVE=1` when `MCP_HOST_CMD` is unset.

## Environment requirements (frozen)
Required for live mode:
- `MCP_HOST_CMD`

Optional for live mode:
- `MCP_HOST_ARGS`
- `ASSAY_CMD` (default: `assay`)

## Variant semantics (frozen)
### Variant A - wrap_only
Enabled:
- wrap policy enforcement

Disabled:
- sequence sidecar

Required live evidence:
- `SIDECAR=disabled`
- selected policy resolves to the wrap-only policy fixture

### Variant B - sequence_only
Enabled:
- sequence sidecar

Disabled for exfil blocking:
- wrap-level deny rules intended to block the sink call

Required live evidence:
- `SIDECAR=enabled`
- selected policy resolves to the sequence-only policy fixture
- enforcement attribution must remain available before the sink call

### Variant C - combined
Enabled:
- wrap policy enforcement
- sequence sidecar

Required live evidence:
- `SIDECAR=enabled`
- selected policy resolves to the combined policy fixture

## Logging contract (frozen)
Every protected live run must emit auditable markers for:
- ablation mode
- sidecar enabled/disabled state
- selected Assay policy
- selected MCP host command

These markers must be present in protected run logs even if a run terminates early.

## Security contract (frozen)
The live-enable implementation must not:
- hardcode absolute user paths in repo-tracked scripts
- automatically execute live runs in CI
- log raw sensitive document bodies into repo artifacts
- weaken the current canary-based scoring contract

## Evidence requirements
Per mode, live runs must still preserve:
- baseline/protected logs
- per-mode `summary.json`
- aggregate `ablation-summary.json`
- enough log markers to attribute whether blocking came from wrap-only behavior, sequence enforcement, or both

## Acceptance criteria (StepA)
- live/offline mode semantics are unambiguous
- live mode requires `MCP_HOST_CMD`
- variant semantics remain identical to Step1/Step2
- logging invariants and security constraints are explicit
- no runtime/workflow changes are part of this slice
