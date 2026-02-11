# PR Strategy: ADR-024 Sim Engine Hardening

**Branch:** `feat/adr-024-sim-hardening`
**ADR:** [ADR-024](./ADR-024-Sim-Engine-Hardening.md)

---

## Recommendation: 2 PRs

| PR | Epics | Scope | Rationale |
|----|-------|-------|------------|
| **PR 1** | E1 | VerifyLimitsOverrides | Foundation; no behavior change; merge early to avoid drift |
| **PR 2** | E2–E6 (+E7) | CLI + Suite + Integrity + attacks + UX | End-to-end feature; coherent user-facing change |

---

## PR 1: Epic 1 (Foundation)

**Scope:** `VerifyLimitsOverrides` in assay-evidence only.

**Why now:**
- Fully tested, review-pack verified
- No user-facing behavior change
- Reduces branch drift; keeps main in sync
- Unblocks E2–E7 (assay-sim/assay-cli can depend on the type)

**Size:** ~180 lines (struct + impl + 4 tests + VERIFICATION doc)

**Merge criteria:** VERIFICATION-ADR-024-E1.md checklist + CI green.

---

## PR 2: Epics 2–6 (Feature)

**Scope:** CLI flags, tier-default limits, configurable TimeBudget, integrity attacks with limits, limit_bundle_bytes, report metadata, budget-exceeded UX.

**Why batched:**
- E2 (CLI) without E3/E4 = flags that don’t do anything
- E2+E3+E4 = minimal “limits work” unit
- E5 adds visible regression-proof attack
- E6 adds UX polish; E7 (print-config + test plan) fits naturally

**Depends on:** PR 1 merged.

**Size:** Est. 400–600 lines across assay-cli, assay-sim.

---

## Not Recommended

- **Single PR (E1–E7):** Large review, higher merge conflicts, harder bisect
- **PR per epic (7 PRs):** E2 alone adds dead CLI surface; excessive overhead
- **E1 with E2:** E2 needs E3/E4 to be useful; partial batch adds confusion
