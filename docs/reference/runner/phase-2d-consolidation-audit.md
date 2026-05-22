# Phase 2D Consolidation Audit

> Snapshot, not roadmap. Records the current verdict after Phase 2D
> Slices 1-6B landed (commit `132c7011`, 2026-05-22): the four named
> structural extraction blockers are resolved, and Slice 7 (repository
> extraction) is still closed. This page replaces the passive 4-6 week
> calendar wait with explicit burn-in criteria.

**The consolidation window is an evidence requirement, not a calendar
requirement.**

Phase 2D Slice 7 may not open until either the burn-in criteria below
are satisfied, or this document is updated to explain why a different
form of evidence is sufficient. Counting weeks is not evidence.

## Status

| Item | Verdict |
|---|---|
| Phase 2D structural blocker #1 (schema crate) | **resolved** (Slice 1, `assay-runner-schema`) |
| Phase 2D structural blocker #2 (archive boundary) | **resolved** (Slices 1 + 2, `assay-runner-core`) |
| Phase 2D structural blocker #3 (cgroup API) | **resolved** (Slice 3, `assay-runner-linux`) |
| Phase 2D structural blocker #4 (`assay-cli` no longer consumes spike) | **resolved** (Slice 6B) |
| Slice 4 (platform composition boundary) | **landed re-scoped** as docs-only freeze; `PlatformAdapter` trait deferred |
| Slice 5A/5B (fixture package boundary) | **landed** under `runner-fixtures/` |
| Slice 6A (external-consumer design note) | **landed** ([`assay-consumes-runner-external.md`](assay-consumes-runner-external.md)) |
| Slice 7 (repository extraction) | **still closed**; see hard gates in [`extraction-roadmap.md` § Slice 7](extraction-roadmap.md#slice-7-repository-split-gated) |
| External consumer demand | **none proven**; no party has asked to consume `assay.runner.*.v0` without depending on Assay |

The 15 [`Extraction Readiness Criteria`](boundary-map.md#extraction-readiness-criteria)
and the 11 [`Extraction Blocking Conditions`](boundary-map.md#extraction-blocking-conditions)
remain the authoritative checklist. This document does not replace
them. It records the current verdict per category.

## Why Not A Passive 4-6 Week Wait

The original boundary-map rule says: if the boundary remains materially
unstable after a 4-6 week consolidation window, treat that as evidence
that extraction is premature. That rule was written assuming the repo
would see normal churn during the window, and that the window would
surface boundary stress through actual maintenance.

Without an external consumer and without organic Runner-impacting
maintenance, calendar time produces no signal. Sitting on a stable
boundary for six idle weeks is not the same as proving the boundary
under load. Passive calendar wait is weak evidence by itself.

The correction is not to weaken the discipline; it is to make the
evidence concrete.

## Burn-In Criteria (Replace The Calendar Window)

Slice 7 may not open until **all** of the following are observed in
the existing monorepo. Each is a verifiable repo-behavior check, not a
time check.

1. **At least two normal (non-Runner) PRs land** that do not require
   any edits to `crates/assay-runner-schema/`,
   `crates/assay-runner-core/`, `crates/assay-runner-linux/`, the
   `assay-runner-spike` wrapper, or the `runner-fixtures/` package
   tree. This proves the new boundary does not leak into unrelated
   work.
2. **At least one Runner-impacting maintenance PR lands** through the
   per-PR discipline rule in
   [`extraction-roadmap.md` § Per-PR Discipline Rule](extraction-roadmap.md#per-pr-discipline-rule),
   without reintroducing `assay-runner-spike` as a dependency of
   `crates/assay-cli/` and without adding a new public API to the
   three runner crates solely to make the maintenance possible.
3. **All existing gates remain green on that maintenance PR.** That
   means: `assay_runner_lane_check.py` classifier and self-test,
   delegated `gates=all` proof on `assay-bpf-runner`, S5
   mcp-policy-agent acceptance, and the Gemini second-runtime
   fixture.
4. **No new public API is added to `assay-runner-schema`,
   `assay-runner-core`, or `assay-runner-linux` to absorb a normal
   bug fix.** If a normal fix requires widening the public surface,
   that is a churn signal and resets the burn-in.
5. **No reintroduction of `assay-runner-spike` as a non-internal
   consumer.** The wrapper stays as a legacy alias; nothing new
   imports through it. The lane-check mechanical absence check
   continues to enforce this.
6. **The boundary-map ownership table is not amended during the
   burn-in** except to record evidence (e.g. marking a row as
   exercised by a specific PR). Structural amendments reset the
   burn-in.

A burn-in clock does not run. There is no minimum number of weeks. If
two normal PRs and one maintenance PR all satisfy criteria 3-6 within
two weeks, the burn-in is satisfied. If they take three months because
no such PRs naturally arise, the burn-in is still incomplete and Slice
7 stays closed.

## Allowed Work During Burn-In

The burn-in window is not a freeze. The following work is explicitly
allowed, and counts toward the burn-in evidence when it occurs:

- [`#1271`](https://github.com/Rul1an/assay/issues/1271) — non-canonical
  ring-buffer diagnostic projection. A natural Type A or Type B
  candidate that exercises `assay-runner-schema` and/or
  `assay-runner-core` boundaries on a real feature.
- Docs hygiene PRs: anchor fixes, mkdocs strict warnings, dead-link
  cleanup, typo fixes in the runner reference.
- CI guardrails: new lane-check self-test scenarios, tighter regexes,
  encoding pinning, additional mechanical absence checks.
- Bug fixes inside any existing runner crate that do not widen the
  public API.
- Acceptance-gate maintenance: re-recording delegated proof on a
  newer self-hosted host, refreshing Gemini fixture trace bytes, S5
  policy-agent maintenance.

These do not require the audit document to be re-opened. They count
as ordinary work.

## Forbidden Work During Burn-In

The following must not be opened during the burn-in window. Each is
forbidden because it either pre-empts Slice 7, makes a product claim
that has no evidence behind it, or relitigates a closed decision.

- Repository split, repository name selection, new repo skeleton, or
  any extraction-side scaffolding under a different `git remote`.
- Publication: `publish = true` on any of the three runner crates,
  crates.io reservation, npm/PyPI reservation, or registry presence.
- macOS or Windows measurement work. These are governed exclusively
  by [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md#macos-proof-readiness)
  and require their own spike plan.
- A third runtime spike beyond the Linux + Gemini second-runtime
  scope already accepted in
  [`second-runtime-plan.md`](second-runtime-plan.md).
- Reopening the `PlatformAdapter` trait or any platform-composition
  façade before one of the three reopen triggers in
  [`extraction-roadmap.md` § Slice 4](extraction-roadmap.md#slice-4-platform-composition-boundary-landed-re-scoped)
  fires.
- New `assay.runner.*.v1` contract work. The v0 contracts are still
  under churn observation.
- Public marketing of Assay-Runner as a standalone product (homepage
  hero, "available now", external announcement, conference talk
  framing it as a separate release).

## Decision Checkpoint

After the burn-in criteria are satisfied, this document is updated
with one of three verdicts:

1. **Keep in monorepo, publish-disabled.** Default outcome if no
   external use case has been articulated. The three runner crates
   stay `publish = false`; the spike wrapper stays as legacy alias;
   the boundary discipline stays in force. Slice 7 stays closed and
   this document is re-issued for the next burn-in cycle.
2. **Open Slice 7 planning issue.** Only if (a) burn-in is satisfied
   *and* (b) at least one concrete external consumer use case is
   documented in a GitHub Discussion or issue. The Slice 7 planning
   issue then names the repository, license, branch protection, CI
   surface, and publication target.
3. **Reset consolidation due to churn.** If burn-in criterion 4, 5,
   or 6 is violated during the window, the burn-in is reset and the
   reason is recorded here. This is not a failure; it is the
   document doing its job.

The decision belongs in a docs-only update to this page. It does not
open implementation work by itself.

## Non-Claims

This document does not:

- claim Assay-Runner is a standalone product
- claim external demand exists or is being measured
- claim a release window for Slice 7
- announce a repository name or layout
- replace any of the 15 readiness criteria or the 11 blocking
  conditions
- weaken any existing acceptance gate (lane-check, delegated,
  Gemini, S5)
- authorize any of the work listed under Forbidden Work
- supersede [`boundary-map.md`](boundary-map.md),
  [`extraction-roadmap.md`](extraction-roadmap.md), or
  [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md);
  it sits next to them as the consolidation-evidence page

## References

- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Assay-Runner extraction roadmap (Phase 2D slice sequence)](extraction-roadmap.md)
- [Runner platform and extraction readiness](platform-and-extraction-readiness.md)
- [Assay consumes Runner as external — Slice 6A design note](assay-consumes-runner-external.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
