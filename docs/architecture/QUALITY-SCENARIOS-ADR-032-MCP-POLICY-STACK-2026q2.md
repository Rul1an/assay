# ADR-032 Quality Scenarios (2026 Q2)

> Status: Current-state quality scenarios after Wave42
> Canonical ADR: [ADR-032](./ADR-032-MCP-Policy-Obligations-and-Evidence-v2.md)
> Structural view: [Building Block View](./BUILDING-BLOCKS-ADR-032-MCP-POLICY-STACK-2026q2.md)

This page makes the architecture quality goals of the ADR-032 line explicit.
It is intentionally scenario-based so maintainers can review future changes against concrete expectations instead of vague design intent.

## How To Read These Scenarios

Each scenario uses the same shape:

- source: who or what triggers the event
- stimulus: what happens
- environment: under what conditions
- expected response: how Assay should behave
- evidence of success: how we know the architecture still satisfies the requirement

## Q1. Deterministic Replay

**Source:** maintainer, CI, or replay consumer

**Stimulus:** re-run replay/diff against the same evidence and policy basis

**Environment:** no runtime capability change, only repeated replay/read activity

**Expected response:**
- replay classification stays deterministic
- diff basis does not depend on reader guesswork
- deny origin and compatibility markers remain stable

**Evidence of success:**
- replay/diff payloads compare deterministically in CI
- reader precedence does not change output interpretation silently

## Q2. Additive Consumer Compatibility

**Source:** downstream CLI/report/reporting consumer

**Stimulus:** read a newer decision/evidence payload after compatibility hardening waves

**Environment:** consumer may still use an older or partial read path

**Expected response:**
- payload evolution is additive where possible
- compatibility markers are explicit
- consumer precedence is deterministic

**Evidence of success:**
- older readers do not require runtime re-derivation of semantics
- compatibility fields remain documented and testable

## Q3. Typed Fail-Closed Safety

**Source:** runtime fault, missing context, or fail-closed trigger

**Stimulus:** evaluation cannot safely continue under the active fail-closed contract

**Environment:** risk/tool class requires bounded fallback behavior

**Expected response:**
- fail-closed behavior is typed and explicit
- evidence separates fail-closed deny from policy deny and enforcement deny
- deny reasons remain deterministic

**Evidence of success:**
- fail-closed denial paths are visible in decision/evidence payloads
- downstream replay can distinguish fallback origin cleanly

## Q4. Bounded Runtime Evolution

**Source:** maintainer introducing a new capability

**Stimulus:** a new enforcement path or obligation is proposed

**Environment:** post-Wave42 architecture line

**Expected response:**
- new capability begins with a bounded contract freeze
- runtime behavior does not widen implicitly inside hardening slices
- ownership boundary between evaluator, enforcement, and evidence stays clear

**Evidence of success:**
- capability work lands as explicit new waves
- hardening slices remain behavior-preserving unless explicitly stated otherwise

## Q5. Auditability and Evidence Reconstruction

**Source:** reviewer, auditor, or incident analyst

**Stimulus:** reconstruct how a decision was reached and what runtime path was taken

**Environment:** decision, obligation, replay, and context payloads available

**Expected response:**
- decision and enforcement origins are visible
- obligation outcomes are normalized
- context completeness is explicit
- evidence is sufficient to replay classification and consumer interpretation

**Evidence of success:**
- decision stream and replay basis explain the outcome without external guesswork
- approval/scope/redaction/fail-closed semantics can be reconstructed from emitted evidence

## Q6. Context Completeness Robustness

**Source:** runtime caller or downstream reader

**Stimulus:** a payload has partial or absent envelope fields

**Environment:** `lane`, `principal`, `auth_context_summary`, or `approval_state` may be incomplete

**Expected response:**
- completeness is explicit, not inferred ad hoc
- missing fields are visible as metadata
- reader logic can remain deterministic despite partial envelopes

**Evidence of success:**
- context completeness metadata is emitted consistently
- consumers can distinguish complete, partial, and absent envelope states

## Q7. Release and Semver Clarity

**Source:** downstream crate user or integration maintainer

**Stimulus:** upgrade to a newer release line with expanded evidence/replay payloads

**Environment:** public or semipublic payload contract changed additively

**Expected response:**
- release notes frame consumer impact explicitly
- intended public shape changes are documented as such
- semver-sensitive payload evolution is not treated as invisible internal refactoring

**Evidence of success:**
- release notes and ADR notes describe widened payload contracts
- hygiene follow-ups do not silently redefine public surface area

## Q8. Documentation Routing Clarity

**Source:** maintainer updating the architecture line

**Stimulus:** a change affects decision meaning, current shape, rollout history, or consumer framing

**Environment:** multiple architecture documents exist for different roles

**Expected response:**
- maintainers can tell whether to update ADR, overview, plan, or release notes
- documentation drift is minimized
- the repo remains the single source of truth

**Evidence of success:**
- building blocks, quality scenarios, overview, plan, and release notes each keep a clear role
- Obsidian remains a view layer instead of a competing source of truth

## Review Checklist

Use these questions during future reviews:

1. Does this change reduce determinism for replay, consumer reads, or deny classification?
2. Does this change widen runtime behavior without an explicit bounded wave?
3. Does this change blur policy deny, fail-closed deny, and enforcement deny?
4. Does this change require new compatibility markers or consumer precedence notes?
5. Does this change need release-note framing because downstream consumers can observe it?
