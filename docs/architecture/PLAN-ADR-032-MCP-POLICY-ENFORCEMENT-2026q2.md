# Plan: ADR-032 MCP Policy Enforcement and Evidence (2026 Q2)

> Status: Closed loop through Wave42 on `main`
> Date: 2026-03-15
> Scope: MCP runtime policy decisions, obligations, and decision evidence
> Constraint: Bounded slices, deterministic behavior, backward-compatible event evolution
> Note: This page is the historical rollout log. For the current architecture view, see [ADR-032 Implementation Overview](./OVERVIEW-ADR-032-MCP-POLICY-STACK-2026q2.md).

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
| Wave33 | obligation outcome normalization | merged |
| Wave34 | fail-closed matrix typing | merged |
| Wave35 | obligation fulfillment normalization | merged |
| Wave36 | redact enforcement hardening | merged |
| Wave37 | decision/evidence convergence | merged |
| Wave38 | replay diff contract | merged |
| Wave39 | evidence compatibility normalization | merged |
| Wave40 | deny evidence convergence | merged |
| Wave41 | consumer hardening | merged |
| Wave42 | context-envelope hardening | merged |

## 4) What Is Stable Now

- typed decision contract (`allow`, `allow_with_obligations`, `deny`, `deny_with_alert`)
- additive decision/event evidence model
- bounded execution paths for `log`, `alert`, `approval_required`, `restrict_scope`, `redact_args`
- deterministic deny reasons for bounded enforcement failures
- normalized obligation fulfillment evidence with stable `reason_code`, `enforcement_stage`, and `normalization_version`
- typed fail-closed evidence and deny/evidence convergence
- replay/diff basis, compatibility fields, and downstream consumer read precedence
- additive context-envelope completeness metadata for `lane`, `principal`, `auth_context_summary`, and `approval_state`

## 5) Post-Wave42 Follow-Up (Bounded)

### F1: Compatibility maintenance
- keep additive compatibility paths explicit while downstream consumers migrate to normalized fields
- avoid silent removal of legacy-read surfaces without an explicit deprecation wave

### F2: Replay and evidence ergonomics
- keep replay diagnostics deterministic for policy revision comparisons
- keep deny and context payload readers aligned with the normalized evidence line

### F3: New capability work only as bounded waves
- any future runtime capability should start with a new contract freeze
- do not reopen the ADR-032 line with incidental runtime behavior changes hidden inside hardening slices

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

As of Wave42, these criteria are satisfied on `main` for the shipped MCP policy/enforcement stack.
