# Plan: ADR-032 MCP Policy Enforcement and Evidence (2026 Q2)

> Status: Closed loop through Wave32 on `main`
> Date: 2026-03-13
> Scope: MCP runtime policy decisions, obligations, and decision evidence
> Constraint: Bounded slices, deterministic behavior, backward-compatible event evolution

## 1) Goal
Deliver a production-safe evolution from tool-call gating to typed policy enforcement with obligations and replayable evidence, without taking ownership of identity/token infrastructure.

## 2) Architecture Guardrails
Always enforce:

- Assay is not an IdP/OAuth/token broker
- transport auth remains external; Assay consumes context
- no broad workflow/control-plane expansion in runtime slices
- no big-bang evaluator rewrite

## 3) Wave Timeline (Completed)

| Wave | Focus | Result on `main` |
|---|---|---|
| Wave24 | typed decisions + Decision Event v2 | merged |
| Wave25 | `log` obligation execution | merged |
| Wave26 | `alert` obligation execution | merged |
| Wave27 | approval artifact/data shape | merged |
| Wave28 | `approval_required` enforcement | merged |
| Wave29 | `restrict_scope` shape/evidence | merged |
| Wave30 | `restrict_scope` enforcement | merged |
| Wave31 | `redact_args` shape/evidence | merged |
| Wave32 | `redact_args` enforcement | merged |

## 4) What Is Stable Now

- typed decision contract (`allow`, `allow_with_obligations`, `deny`, `deny_with_alert`)
- additive decision/event evidence model
- bounded execution paths for `log`, `alert`, `approval_required`, `restrict_scope`, `redact_args`
- deterministic deny reasons for bounded enforcement failures

## 5) Immediate Follow-Up (Bounded)

### F1: Obligation fulfillment normalization
- standardize fulfillment/outcome shape across all executed obligations
- keep additive event compatibility for legacy consumers

### F2: Fail-closed matrix typing
- explicit tool-class/risk defaults in runtime contract
- deterministic error path when evaluator/context dependencies fail

### F3: Replay and diff hardening
- improve replay diagnostics for policy revision comparisons
- keep evidence deterministic and policy-version keyed

## 6) Explicit Non-Goals for Follow-Up

- no OAuth/IdP feature work
- no approval UI/case-management product work
- no external incident integrations as mandatory runtime dependencies
- no policy backend replacement as part of hardening slices

## 7) Exit Criteria for ADR-032 Line
ADR-032 execution line is considered operationally complete when:

1. all bounded obligation executions emit normalized fulfillment evidence,
2. fail-closed behavior is typed and deterministic by risk/tool class,
3. replay/diff on policy revisions is stable in CI and evidence bundles,
4. compatibility paths are either retained with explicit guarantees or deprecated by explicit wave.
