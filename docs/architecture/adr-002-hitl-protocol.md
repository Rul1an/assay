# ADR 002: Human-in-the-Loop (HITL) as a Protocol Boundary

**Date**: 2026-01-26
**Status**: Proposed

## Context
Assay needs to support human oversight for high-risk actions (e.g., file deletion, shell execution). However, traditional interactive prompts in the sandbox loop break determinism, cause issues in CI, and duplicate UI logic that should reside in the host (e.g., Cursor, IDE).

## Decision
We define HITL as a **protocol/decision boundary** rather than a UI component.

1.  **"RequiresApproval" Decision**: The policy engine can return a `PolicyDecision::RequiresApproval` variant containing the `request_id`, `tool_id`, `args_fingerprint`, and `risk_class`.
2.  **Structured Event Sink**: Events are emitted via a JSONL sink (e.g., `--events-path`) to be consumed by the host or observability tools.
3.  **CI/Headless Contract**:
    - In non-interactive/CI mode, a required approval results in `exit 2` and the `E_APPROVAL_REQUIRED` marker.
    - Determinism is maintained via `--approvals <FILE>` which provides pre-authorized decisions for specific fingerprints.
4.  **Ownership**: Hosts (IDE) are responsible for user presentation; Assay is responsible for policy evaluation and block-signals.

## Rationale
- **Determinism**: Decoupling the wait-for-human from the core loop ensures replays remain stable.
- **CI Friendly**: Standardizing exit codes and markers allows pipelines to treat approvals as "security gates".
- **Separation of Concerns**: Assay avoids building UI logic for every possible host.

## Consequences
- Hosts must implement a listener for Assay events if they want to support live approvals.
- Replays must include approved fingerprints in their trace to be fully deterministic.

## Non-Goals
- No interactive blocking prompts in the core `assay-cli` sandbox loop (by default).
- No blocking "wait for human" timeouts inside the unprivileged executor.
