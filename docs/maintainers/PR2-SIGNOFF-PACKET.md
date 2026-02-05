# PR2 (E2.3 SARIF limits) – Sign-off packet

**Branch:** `feat/e2.3-sarif-limits`
**Doel:** Audit-grade bewijsstukken voor sign-off (diff-snippets, output samples, contract asserts, edge checks, SPEC).

---

## 1) Diff-snippets (3 plekken)

### 1.1 assay-core `report/sarif.rs`

**Eligibility + ranking (pure functions):**

```rust
/// Whether this status is included in SARIF output (deterministic truncation contract).
/// Eligible: Fail, Error, Warn, Flaky, Unstable. Excluded: Pass, Skipped, AllowedOnError.
#[inline]
pub fn is_sarif_eligible(status: TestStatus) -> bool {
    !matches!(
        status,
        TestStatus::Pass | TestStatus::Skipped | TestStatus::AllowedOnError
    )
}

/// Blocking rank for truncation order: 0 = blocking (Fail/Error), 1 = non-blocking (Warn/Flaky/Unstable).
#[inline]
pub fn blocking_rank(status: TestStatus) -> u8 {
    if status.is_blocking() { 0 } else { 1 }
}

/// Severity rank for SARIF truncation: 0 = error, 1 = warning, 2 = note.
#[inline]
pub fn severity_rank(status: TestStatus) -> u8 {
    match status {
        TestStatus::Fail | TestStatus::Error => 0,
        TestStatus::Warn | TestStatus::Flaky | TestStatus::Unstable => 1,
        _ => 2,
    }
}

/// Sort key for deterministic truncation: (BlockingRank, SeverityRank, test_id). Stable and input-order independent.
fn sarif_sort_key(r: &TestResultRow) -> (u8, u8, &str) {
    (blocking_rank(r.status), severity_rank(r.status), r.test_id.as_str())
}
```

**Filter → sort → take + omitted_count (eligible_total − included) + runs[0].properties.assay:**

```rust
    let eligible: Vec<&TestResultRow> = results.iter().filter(|r| is_sarif_eligible(r.status)).collect();
    let eligible_total = eligible.len();

    let mut sorted: Vec<&TestResultRow> = eligible;
    sorted.sort_by_cached_key(|r| sarif_sort_key(*r));
    let kept: Vec<&TestResultRow> = sorted.into_iter().take(max_results).collect();
    let kept_count = kept.len();
    let omitted_count = eligible_total.saturating_sub(kept_count) as u64;

    // ... build sarif_results from kept ...

    let run_obj: serde_json::Value = if omitted_count > 0 {
        serde_json::json!({
            "tool": { "driver": { "name": tool_name } },
            "results": sarif_results,
            "properties": {
                "assay": {
                    "truncated": true,
                    "omitted_count": omitted_count
                }
            }
        })
    } else {
        serde_json::json!({
            "tool": { "driver": { "name": tool_name } },
            "results": sarif_results
        })
    };
```

---

### 1.2 assay-core `report/summary.rs`

**Definitie `sarif: Option<SarifOutputInfo>`:**

```rust
    /// SARIF truncation (E2.3). Present when SARIF was truncated (N results omitted).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sarif: Option<SarifOutputInfo>,
}

/// SARIF output metadata (E2.3). Written when SARIF was truncated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SarifOutputInfo {
    /// Number of results omitted from SARIF due to max_results limit.
    pub omitted: u64,
}
```

**with_sarif_omitted (alleen zetten als > 0):**

```rust
    /// Set SARIF truncation info (E2.3). Call when omitted_count > 0.
    pub fn with_sarif_omitted(mut self, omitted: u64) -> Self {
        if omitted > 0 {
            self.sarif = Some(SarifOutputInfo { omitted });
        }
        self
    }
```

---

### 1.3 assay-cli `write_extended_run_json`

**Injectie `sarif: { omitted: N }` alleen bij truncatie (n > 0):**

```rust
        // E2.3: SARIF truncation metadata when SARIF was truncated
        if let Some(n) = sarif_omitted {
            if n > 0 {
                obj.insert(
                    "sarif".to_string(),
                    serde_json::json!({ "omitted": n }),
                );
            }
        }
```

`write_run_json_minimal` (early-exit pad) krijgt geen `sarif_omitted` en schrijft geen `sarif` key — correct, want bij early exit wordt geen SARIF geschreven.

---

## 2) Eén echte output sample (contracttest-equivalent)

Contracttest: `write_sarif_with_limit("assay", &results, &path, 10)` met 25× Fail (test_00 … test_24). Verwacht: omitted=15, included=10.

**SARIF fragment (relevante keys):**

```json
{
  "runs": [{
    "tool": { "driver": { "name": "assay" } },
    "results": [ /* 10 items: test_00 … test_09 */ ],
    "properties": {
      "assay": {
        "truncated": true,
        "omitted_count": 15
      }
    }
  }]
}
```

- `runs[0].properties.assay.truncated` == `true`
- `runs[0].properties.assay.omitted_count` == `15`
- `runs[0].results.length` == `10`

**run.json fragment (alleen bij truncatie, uit CLI-flow):**

```json
"sarif": { "omitted": 15 }
```

**summary.json fragment (alleen bij truncatie):**

```json
"sarif": { "omitted": 15 }
```

---

## 3) Contract test: exacte asserts + testnaam + bouw 25 results

**Bestand:** `crates/assay-core/tests/contract_sarif.rs`
**Testnaam:** `test_sarif_truncation_properties`

**Bouw 25 resultaten (alle eligible, geen Pass/Skipped/AllowedOnError):**

```rust
    const MAX_RESULTS: usize = 10;
    let n_eligible = 25;
    let results: Vec<TestResultRow> = (0..n_eligible)
        .map(|i| TestResultRow {
            test_id: format!("test_{:02}", i),
            status: TestStatus::Fail,
            // ... score: None, cached: false, message: "fail", ...
        })
        .collect();
    assert_eq!(
        results.iter().filter(|r| is_sarif_eligible(r.status)).count(),
        n_eligible
    );
```

**Asserts:**

- **omitted == eligible - included:**
  `assert_eq!(outcome.omitted_count, expected_omitted)` met `expected_omitted = (n_eligible - MAX_RESULTS) as u64` (= 15).
- **Keys en types (truncated, omitted_count):**
  `assert!(props.is_some())`;
  `assert_eq!(props.unwrap()["truncated"], true)`;
  `assert_eq!(props.unwrap()["omitted_count"], expected_omitted)`.
- **Aantal results:**
  `assert_eq!(results_arr.len(), MAX_RESULTS)` (10).
- **Deterministische volgorde (eerste = laagste test_id):**
  `assert!(first_msg.starts_with("test_00:"), "first result must be lowest test_id (deterministic sort): got {}", first_msg)`.

Geen non-eligible meegeteld: alle 25 resultaten zijn `TestStatus::Fail` en de test checkt expliciet `filter(|r| is_sarif_eligible(r.status)).count() == n_eligible`.

---

## 4) Edge checks

### 4.1 eligible_total <= limit → geen truncatie

- **Keuze:** `properties.assay` **ontbreekt** (we schrijven alleen de `else`-tak: `run_obj` zonder `"properties"`).
- **sarif in run/summary:** Ontbreekt.
  - CLI: `if let Some(n) = sarif_omitted { if n > 0 { obj.insert("sarif", ...) } }` → bij omitted_count 0 wordt geen `sarif` gezet.
  - Summary: `with_sarif_omitted(omitted)` zet alleen `self.sarif = Some(...)` als `omitted > 0`.

**Bewijs (test):** `test_sarif_no_truncation_under_limit` — 1× Fail, `write_sarif` (default limit).
Asserts: `outcome.omitted_count == 0`; `run.get("properties").is_none()`.

### 4.2 0 eligible (alleen Pass/Skipped/AllowedOnError)

- **SARIF:** Geldig bestand met 0 results.
  - `eligible` = []; `eligible_total` = 0; `sorted` = []; `kept` = []; `sarif_results` = []; `omitted_count` = 0.
  - We nemen de `else`-tak: `run_obj` zonder `"properties"`.
  - Output: `runs[0].results` = `[]`, geen `properties.assay`.
- **Geen truncation metadata:** Geen `properties.assay` (omitted_count is 0).

Geen aparte test; gedrag volgt direct uit de code (filter levert lege lijst → geen properties, lege results).

---

## 5) SPEC-diff (§6.3)

**Relevante stukken uit SPEC-PR-Gate-Outputs-v1.md §6.3:**

- **Eligibility / omitted_count:**
  "**Eligibility:** only SARIF-eligible results (e.g. Fail, Error, Warn, Flaky, Unstable) count toward the limit; `omitted_count` = eligible_total − included."
- **Types run-level metadata:**
  "`runs[].properties.assay` MUST be present when truncation was applied:
  - `truncated` (boolean): `true`
  - `omitted_count` (integer): number of eligible results omitted"
- **Schema run/summary:**
  "`sarif` (object, optional): present only when truncation occurred.
  `sarif.omitted` (integer, required when `sarif` is present): ≥ 1."
- **Normatieve zin:**
  "**Consumers MUST treat SARIF as potentially truncated and MUST use summary/run for authoritative counts.**"

---

## Test-output checklist

```bash
cargo test -p assay-core --test contract_sarif
```

**Actuele output:**

```
running 4 tests
test test_invariant_sarif_always_has_locations ... ok
test test_sarif_no_truncation_under_limit ... ok
test test_sarif_mixed_status_ordering ... ok
test test_sarif_truncation_properties ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

---

## PR review comment (copy/paste voor in de PR)

Overall looks solid: deterministic filter/sort/take, SARIF run-level truncation metadata, and sarif.omitted in run/summary gated on omitted>0.

**Done:** omitted_count is now computed as eligible_total − included_count (kept.len()) so it stays correct if result-mapping ever drops items. Mixed-status ordering test added; SPEC defines eligible_total/included and adds invariant that sarif.omitted MUST equal runs[0].properties.assay.omitted_count when both present.
