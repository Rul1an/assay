# P2: E2.3 SARIF limits – uitvoeringsplan

**Branch:** `feat/e2.3-sarif-limits`
**Issue:** #104 (E2.3 SARIF limits: truncate + N omitted)
**DoD (OPEN-ITEMS-EXECUTION-PLAN §2.4):** Max resultaten (25_000), bij truncatie "N results omitted"; deterministische selectie; `properties.assay` in SARIF; `sarif.omitted` in summary/run.

**Review (SOTA Feb 2026):** Eligibility/ranking expliciet; filter→sort→take; wrapper API; nested `sarif.omitted`; cheap contract test.

---

## 1. Eligibility en ranking (blockers)

- **SarifEligible:** Pure functie op `TestStatus`. Eligible = Fail, Error, Warn, Flaky, Unstable. Excluded = Pass, Skipped, AllowedOnError. (`is_sarif_eligible` in code.)
- **BlockingRank:** 0 = blocking (Fail/Error), 1 = non-blocking (Warn/Flaky/Unstable). Policy-proof voor E7.4.
- **SeverityRank:** 0 = error, 1 = warning, 2 = note. Sort key = (BlockingRank, SeverityRank, test_id) — alleen stabiele velden, input-order onafhankelijk.

---

## 2. Truncatie (assay-core `report/sarif.rs`)

- **Flow:** Filter → sort → take. Eerst filteren op eligible, dan sorteren op (BlockingRank, SeverityRank, test_id), dan eerste `max_results` behouden. `omitted_count` = eligible_total − included (alleen eligible tellen).
- **API (wrapper):**
  - `write_sarif(tool_name, results, out) -> Result<SarifWriteOutcome>` — default limit [`DEFAULT_SARIF_MAX_RESULTS`], minimale signature-churn op call sites.
  - `write_sarif_with_limit(tool_name, results, out, max_results: usize) -> Result<SarifWriteOutcome>` — voor tests en custom limits.
- **SARIF run-level:** Bij truncatie `runs[0].properties.assay.truncated` en `runs[0].properties.assay.omitted_count`. Geen size-cap in deze PR (TODO/follow-up).

---

## 3. Summary + run.json: nested `sarif`

- **Schema:** Eén vorm overal: `sarif: { omitted: N }` (nested object). Niet `sarif_omitted` top-level (future-proof voor bv. sarif.limit, sarif.bytes).
- **Normatief:** Alleen aanwezig bij truncatie (omitted ≥ 1); anders geen `sarif` key (geen noise).
- **Summary:** `sarif: Option<SarifOutputInfo>`, `SarifOutputInfo { omitted: u64 }`, `with_sarif_omitted(u64)`.
- **run.json:** Top-level `"sarif": { "omitted": n }` wanneer n > 0.

---

## 4. assay-cli

- Na `write_sarif` outcome gebruiken; `write_extended_run_json(..., sarif_omitted)` en `summary.with_sarif_omitted(omitted_count)`.

---

## 5. Contract test (cheap, deterministisch)

- Gebruik `write_sarif_with_limit(..., max_results=10)`, 25 eligible results → verwacht omitted=15, included=10.
- Asserts: `runs[0].properties.assay.truncated == true`, `omitted_count == 15`, `results.len() == 10`; ordering: eerste result = blocking + laagste test_id (bv. test_00).
- Geen 25k resultaten in CI.

---

## 6. SPEC (normatief, klein)

- run/summary: `sarif` (object, optional), `omitted` (integer, required when present, ≥1).
- SARIF: `runs[].properties.assay.truncated`, `runs[].properties.assay.omitted_count`.
- Eén normatieve zin: "Consumers MUST treat SARIF as potentially truncated and MUST use summary/run for authoritative counts."

---

## Referenties

- OPEN-ITEMS-EXECUTION-PLAN.md §2.4, §3 (batch C), §6 (contract test PR 2), §7 (SPEC).
- DX-IMPLEMENTATION-PLAN: E2.3 DoD.
- SPEC-PR-Gate-Outputs-v1.md §6.3, §3 (sarif object).
