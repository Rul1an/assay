# PR1 Review Packet — E7.5, E7.2, E7.3 (audit-grade)

Voor inhoudelijke sign-off: kernbestanden, drie concrete outputs, testlijst, determinisme, schema-keuzes, precedence-tabel en cardinality-check.

---

## 1) Exacte diff / links naar kernbestanden

*(Diffs: `git diff origin/main...HEAD` op de PR-branch; plus lokale fix voor early-exit seeds.)*

### E7.5 (Judge Uncertain)

**crates/assay-cli/src/exit_codes.rs**

```diff
diff --git a/crates/assay-cli/src/exit_codes.rs b/crates/assay-cli/src/exit_codes.rs
--- a/crates/assay-cli/src/exit_codes.rs
+++ b/crates/assay-cli/src/exit_codes.rs
@@ -90,6 +90,8 @@ pub enum ReasonCode {
     // Test Failure (exit 1)
     /// One or more tests failed
     ETestFailed,
+    /// Judge returned uncertain (abstain) — model could not decide; policy-dependent
+    EJudgeUncertain,
     /// Policy violation detected
     EPolicyViolation,
@@ -133,6 +135,7 @@ impl ReasonCode {
             // V2: Test failures -> 1
             ReasonCode::ETestFailed
+            | ReasonCode::EJudgeUncertain
             | ReasonCode::EPolicyViolation
@@ -172,6 +175,7 @@ impl ReasonCode {
             ReasonCode::ETestFailed => "E_TEST_FAILED",
+            ReasonCode::EJudgeUncertain => "E_JUDGE_UNCERTAIN",
             ReasonCode::EPolicyViolation => "E_POLICY_VIOLATION",
@@ -219,6 +223,10 @@ impl ReasonCode {
             ReasonCode::ETestFailed => "Run: assay explain <test-id> for details".to_string(),
+            ReasonCode::EJudgeUncertain => {
+                "Review borderline result or adjust judge threshold; run: assay explain <test-id>"
+                    .to_string()
+            }
@@ -293,6 +301,23 @@ impl RunOutcome {
         }
     }
+
+    /// Create an outcome when judge returned uncertain (abstain) — exit 1, E_JUDGE_UNCERTAIN
+    pub fn judge_uncertain(abstain_count: usize) -> Self {
+        Self {
+            exit_code: EXIT_TEST_FAILURE,
+            reason_code: ReasonCode::EJudgeUncertain.as_str().to_string(),
+            message: Some(format!(
+                "Judge uncertain (abstain) for {} test(s); cannot decide pass/fail",
+                abstain_count
+            )),
+            next_step: Some(
+                "Review borderline result or adjust judge threshold; run: assay explain <test-id>"
+                    .to_string(),
+            ),
+            warnings: Vec::new(),
+        }
+    }
 }
```

**crates/assay-cli/src/cli/commands/mod.rs — decide_run_outcome volgorde + has_judge_verdict_abstain**

```diff
-    // Priority 3: Test Failures
+    // Priority 3: Judge uncertain (abstain) — exit 1, E_JUDGE_UNCERTAIN
+    let abstain_count = results
+        .iter()
+        .filter(|r| has_judge_verdict_abstain(&r.details))
+        .count();
+    if abstain_count > 0 {
+        let mut o = RunOutcome::judge_uncertain(abstain_count);
+        o.exit_code = ReasonCode::EJudgeUncertain.exit_code_for(version);
+        return o;
+    }
+
+    // Priority 4: Test Failures
     let fails = results ...
-    // Priority 4: Strict Mode Violations
+    // Priority 5: Strict Mode Violations
 ...
-    // Success (ensure version compliance ...)
+    // Priority 6: Success (ensure version compliance ...)
     let mut o = RunOutcome::success();

+/// True if this result row has any judge metric with verdict "Abstain" (E7.5).
+fn has_judge_verdict_abstain(details: &serde_json::Value) -> bool {
+    let Some(metrics) = details.get("metrics").and_then(|m| m.as_object()) else {
+        return false;
+    };
+    for (_name, metric_val) in metrics {
+        if let Some(inner) = metric_val.get("details").and_then(|d| d.get("verdict")) {
+            if inner.as_str() == Some("Abstain") {
+                return true;
+            }
+        }
+    }
+    false
+}
```

---

### E7.2 (Seeds)

**crates/assay-core/src/engine/runner.rs** — RunArtifacts.order_seed

*(Op main staat al: seed default als `cfg.settings.seed.is_none()` → `rand::random()`, shuffle met `StdRng::seed_from_u64(seed)`. PR voegt alleen het veld in de return toe.)*

```diff
         Ok(RunArtifacts {
             run_id,
             suite: cfg.suite.clone(),
             results: rows,
+            order_seed: cfg.settings.seed,
         })
```

**crates/assay-core/src/report/mod.rs** — RunArtifacts

```diff
 pub struct RunArtifacts {
     pub run_id: i64,
     pub suite: String,
     pub results: Vec<TestResultRow>,
+    /// Seed used for test order randomization (E7.2). Present when run used a seed.
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub order_seed: Option<u64>,
 }
```

**crates/assay-core/src/report/summary.rs** — SEED_VERSION, Seeds, JudgeMetrics, with_seeds, judge_metrics_from_results

```diff
+pub const SEED_VERSION: u32 = 1;
 ...
     pub performance: Option<PerformanceMetrics>,
+
+    /// Seeds for deterministic replay (E7.2). Present when run used a seed.
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub seeds: Option<Seeds>,
+
+    /// Judge reliability metrics (E7.3). Present when run had judge evaluations.
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub judge_metrics: Option<JudgeMetrics>,
+}
+
+/// Seeds used in the run (replay determinism)
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct Seeds {
+    pub seed_version: u32,
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub order_seed: Option<u64>,
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub judge_seed: Option<u64>,
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub sampling_seed: Option<u64>,
+}
+
+/// Judge reliability metrics (low cardinality, E8-consistent)
+#[derive(Debug, Clone, Serialize, Deserialize)]
+pub struct JudgeMetrics {
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub abstain_rate: Option<f64>,
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub flip_rate: Option<f64>,
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub consensus_rate: Option<f64>,
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub unavailable_count: Option<u32>,
 }
 ...
+            seeds: None,
+            judge_metrics: None,
         }
     }
 ...
+    /// Set seeds for replay determinism (E7.2). Always set for schema stability (early-exit uses null).
+    pub fn with_seeds(mut self, order_seed: Option<u64>, judge_seed: Option<u64>) -> Self {
+        self.seeds = Some(Seeds {
+            seed_version: SEED_VERSION,
+            order_seed,
+            judge_seed,
+            sampling_seed: None,
+        });
+        self
+    }
+
+    /// Set judge reliability metrics (E7.3)
+    pub fn with_judge_metrics(mut self, metrics: JudgeMetrics) -> Self { ... }
+}
+
+/// Compute judge reliability metrics from run results (E7.3).
+pub fn judge_metrics_from_results(results: &[crate::model::TestResultRow]) -> Option<JudgeMetrics> {
+    // ... telt verdict "Abstain", consensus (a==0||a==1), flip proxy (swapped && 0<a<1),
+    // unavailable_count (Error + timeout/5xx/rate limit/network)
+    ...
+}
```

**crates/assay-cli/src/cli/commands/mod.rs — write_extended_run_json (seeds + judge_metrics), write_run_json_minimal (seed velden), write_error (summary.with_seeds)**

```diff
+        // E7.2: seeds for replay determinism
+        if let Some(seed) = artifacts.order_seed {
+            obj.insert("seed_version", ... SEED_VERSION);
+            obj.insert("order_seed", ... seed);
+            obj.insert("judge_seed", ... seed);
+        }
+
+        // E7.3: judge metrics when present
+        if let Some(metrics) = judge_metrics_from_results(&artifacts.results) {
+            obj.insert("judge_metrics", ...);
+        }
```

*Early-exit (run.json minimal + summary seeds) — lokale fix:*

```diff
-        let summary = summary_from_outcome(&o);
+        let summary = summary_from_outcome(&o).with_seeds(None, None);
         if let Err(e) = assay_core::report::summary::write_summary(&summary, &summary_path) {
```

```diff
 fn write_run_json_minimal(...) {
-    // Minimal JSON for early exits (no artifacts available)
+    // Minimal JSON for early exits (no artifacts available). E7.2: seed fields present for schema stability (null when unknown).
     let v = serde_json::json!({
         "exit_code": outcome.exit_code,
         "reason_code": outcome.reason_code,
         "reason_code_version": ...,
+        "seed_version": assay_core::report::summary::SEED_VERSION,
+        "order_seed": null,
+        "judge_seed": null,
         "resolution": outcome
     });
```

**crates/assay-core/src/report/console.rs** — print_run_footer

```diff
+use crate::report::summary::{JudgeMetrics, Seeds};
+
+/// Print seeds and judge metrics to stderr (E7.2/E7.3 job summary visibility in CI logs).
+pub fn print_run_footer(seeds: Option<&Seeds>, judge_metrics: Option<&JudgeMetrics>) {
+    if let Some(s) = seeds {
+        let order = s.order_seed.map(|n| n.to_string()).unwrap_or_else(|| "—".into());
+        let judge = s.judge_seed.map(|n| n.to_string()).unwrap_or_else(|| "—".into());
+        eprintln!(
+            "Seeds (replay): seed_version={} order_seed={} judge_seed={}",
+            s.seed_version, order, judge
+        );
+    }
+    if let Some(m) = judge_metrics {
+        eprintln!(
+            "Judge metrics: abstain_rate={} flip_rate={} consensus_rate={} unavailable_count={}",
+            abstain, flip, consensus, unavail
+        );
+    }
+}
```

**cmd_run / cmd_ci** — summary.with_seeds + print_run_footer

```diff
     summary = summary.with_results(passed, failed, artifacts.results.len());
+    // E7.2: seeds in summary
+    summary = summary.with_seeds(artifacts.order_seed, artifacts.order_seed);
+    // E7.3: judge metrics
+    if let Some(metrics) = judge_metrics_from_results(&artifacts.results) {
+        summary = summary.with_judge_metrics(metrics);
+    }
     assay_core::report::summary::write_summary(&summary, &summary_path)?;
     assay_core::report::console::print_summary(...);
+    assay_core::report::console::print_run_footer(
+        summary.seeds.as_ref(),
+        summary.judge_metrics.as_ref(),
+    );
```

---

### E7.3 (Judge metrics)

Zie **summary.rs** hierboven: `JudgeMetrics`-struct, `judge_metrics_from_results()` (flip_rate proxy gedocumenteerd in code). Geschreven in: summary (`with_judge_metrics`), run.json extended (`write_extended_run_json`), console (`print_run_footer`).

---

### SPEC — docs/architecture/SPEC-PR-Gate-Outputs-v1.md

```diff
+### 3.3.1 Seeds (E7.2 – Replay Determinism)
+
+A top-level **`seeds`** object (summary.json) and top-level **`seed_version`**, **`order_seed`**, **`judge_seed`** (run.json) SHALL be present for schema stability. On early-exit, seeds may be `null` when unknown; `seed_version` SHALL still be present.
+
+| Field          | Type    | Required | Description |
+|----------------|---------|----------|-------------|
+| `seed_version` | integer | **Yes**  | Version of the seed schema. MUST be `1` for Outputs-v1. Consumers MUST branch on `seed_version`. |
+| `order_seed`   | integer | No       | Seed used for test execution order (shuffle). Null on early-exit when unknown. |
+| `judge_seed`   | integer | No       | Seed used for judge randomization (suite-level). Null on early-exit when unknown. |
+| `sampling_seed`| integer | No       | Optional: reserved for future use. |
+
+**Normative:** run.json (extended and minimal) and summary.json SHALL include `seed_version`; order_seed and judge_seed SHALL be present (integer or null). CLI console SHALL print one line: `Seeds: seed_version=1 order_seed=… judge_seed=…`.
+
+### 3.3.2 Judge Metrics (E7.3)
+
+When the run had judge evaluations, a top-level **`judge_metrics`** object MAY be present:
+
+| Field               | Type    | Required | Description |
+|---------------------|---------|----------|-------------|
+| `abstain_rate`      | number  | No       | Fraction of judge evaluations that returned Abstain (uncertain). |
+| `flip_rate`         | number  | No       | ... (Implementation may use a proxy: swapped and non-unanimous agreement ...) |
+| `consensus_rate`    | number  | No       | Fraction of evaluations where all samples agreed. |
+| `unavailable_count` | integer | No       | Count of runs where judge was unavailable (infra/transport); not counted toward abstain_rate. |
+
+**Normative:** Judge unavailable MUST NOT be counted as Abstain; use `unavailable_count` for that.
 ...
 | schema_version | Date     | Changes |
 |----------------|----------|---------|
-| 1              | 2026-01  | Initial: schema_version, exit_code, reason_code, provenance, next_step, SARIF ... |
+| 1              | 2026-01  | Initial: ... E7.2: seeds (seed_version, order_seed, judge_seed) in summary.json, run.json, and console. E7.3: judge_metrics (...) in summary.json, run.json, and console. |
```

---

## 2) Drie concrete outputs (relevante top-level velden)

**Commando’s (lokaal, ASSAY_EXIT_CODES=v2):**

1. **Uncertain/Abstain**
   - Setup: eval met judge die Abstain kan teruggeven (bijv. borderline band 0.4–0.6), één test die daarin valt.
   - `assay run --config <eval> --trace-file <trace> --strict`
   - **Verwacht**: exit 1, reason_code `E_JUDGE_UNCERTAIN`, seeds aanwezig in run.json/summary.json/console, judge_metrics met abstain_rate > 0.

2. **Judge unavailable (infra)**
   - Setup: judge endpoint onbereikbaar (wrong URL, of mock die 503 geeft).
   - **Verwacht**: exit 3, reason_code `E_JUDGE_UNAVAILABLE` (of E_PROVIDER_5XX/E_TIMEOUT/…). Niet exit 1.

3. **Happy path / normal test failure**
   - Happy: passing tests → exit 0, seeds aanwezig, judge_metrics optioneel (abstain_rate 0 als aanwezig).
   - Test failure: één test Fail (geen Abstain) → exit 1, reason_code `E_TEST_FAILED`, seeds + metrics aanwezig.

**Voorbeeld top-level (conceptueel):**

- **Abstain**: `run.json`: `"exit_code": 1`, `"reason_code": "E_JUDGE_UNCERTAIN"`, `"seed_version": 1`, `"order_seed": <int>`, `"judge_seed": <int>`, `"judge_metrics": { "abstain_rate": 0.xx, ... }`. Console: Seeds-regel + Judge metrics-regel.
- **Judge unavailable**: `run.json`: `"exit_code": 3`, `"reason_code": "E_JUDGE_UNAVAILABLE"` (of andere infra code). Geen downgrade naar 1.
- **Happy**: `"exit_code": 0`, `"reason_code": ""`, seeds aanwezig; **Test failure**: `"exit_code": 1`, `"reason_code": "E_TEST_FAILED"`, seeds + metrics.

*(Concrete CI/lokale logs met echte run.json/summary.json-snippets kunnen hier worden geplakt.)*

---

## 3) Testlijst + “wat dekt wat”

| Test | Dekking |
|------|--------|
| `test_has_judge_verdict_abstain_detects_abstain` | E7.5: detectie Abstain in details.metrics |
| `test_has_judge_verdict_abstain_ignores_pass` | E7.5: Pass telt niet als abstain |
| `test_has_judge_verdict_abstain_no_metrics` | E7.5: geen metrics → geen abstain |
| `test_run_outcome_judge_uncertain` (exit_codes.rs) | E7.5: RunOutcome.judge_uncertain exit/message/next_step |
| (decide_run_outcome volgorde) | Precedence: infra(3) → abstain(1) → test fail(1) via integratie/contract; geen aparte unit “abstain_precedes_test_fail” |
| `contract_e72_seeds_happy_path` | E7.2: run.json + summary.json seed_version + integer order_seed/judge_seed op success |
| `contract_reason_code_trace_not_found_v2` | E7.2 early-exit: run.json seed_version + order_seed/judge_seed null; summary seeds (seed_version) |
| `contract_run_json_always_written_arg_conflict` | run.json altijd geschreven (o.a. early-exit) |
| `test_golden_harness` | Console-output inclusief Seeds-regel (genormaliseerd) |
| `order_determinism::same_seed_same_order` | E7.2:zelfde seed ⇒zelfde shuffle (StdRng) |
| `order_determinism::different_seed_may_differ` | E7.2: andere seed ⇒ andere volgorde |
| Judge metrics (summary/run/console) | E7.3: door run met judge; contract_e72_seeds_happy_path draait met judge → metrics in output |

**Uitgevoerd (voorbeeld):**
- `cargo test -p assay-cli contract_exit_codes`
- `cargo test -p assay-cli test_golden_harness`
- `cargo test -p assay-core same_seed_same_order different_seed_may_differ`
- `cargo test -p assay-cli run_outcome_tests`
- `cargo test -p assay-cli --lib exit_codes::`

---

## 4) Determinisme & bron van seeds (1-paragraaf)

**order_seed** komt uit `cfg.settings.seed` (config `settings.seed`). Als dat ontbreekt: bij start van de run wordt één keer `rand::random()` gegenereerd, in `cfg.settings.seed` gezet en gelogd (“Info: No seed provided. Using generated seed: …”). Daarna wordt diezelfde waarde gebruikt voor shuffle (StdRng::seed_from_u64(seed)) en in RunArtifacts.order_seed. **judge_seed** is in de huidige implementatie dezelfde waarde als order_seed (suite-level); per-test kan de judge een afgeleide seed gebruiken (bijv. hash(suite_seed, test_id)). **Seed ontbreekt**: we falen niet; we genereren en loggen. **Garantie**:zelfde config +zelfde seed(s) ⇒zelfde testvolgorde (StdRng + SliceRandom). Judge-aanroepen zijn verder afhankelijk van model/API; binnen één run is per-test seed vast voor reproduceerbaarheid van randomisatie (bijv. label swap).

---

## 5) Schema-/compatibiliteitskeuzes

- **summary.json**
  - **seeds**: `Option<Seeds>`; veld `seeds` kan ontbreken (skip_serializing_if). Bij run/ci wordt altijd `with_seeds(…)` aangeroepen (ook early-exit met None,None) → object altijd aanwezig met ten minste `seed_version`; order_seed/judge_seed kunnen ontbreken of null (serialisatie Option).
  - **judge_metrics**: `Option<JudgeMetrics>`; alleen aanwezig als er judge-evaluaties zijn. Velden binnen JudgeMetrics zijn Option (abstain_rate etc. kunnen ontbreken).

- **run.json extended**
  - Seeds: seed_version/order_seed/judge_seed alleen geïnjecteerd als `artifacts.order_seed.is_some()`.
  - Judge_metrics: alleen als `judge_metrics_from_results` Some geeft.

- **run.json minimal (early exit)**
  - SPEC §3.3.1: seed_version SHALL present; order_seed/judge_seed present (integer or null). Implementatie zou hier `seed_version`, `order_seed: null`, `judge_seed: null` moeten schrijven voor schema-stabiliteit; controleren op PR-branch.

- **SPEC §3.3.1 / §3.3.2**
  - seed_version: Required (Yes). order_seed/judge_seed: “present (integer or null)”, niet “Required” in de tabel; normative tekst: “SHALL be present”.
  - Judge metrics: alle velden “No” (optional).

- **Onbekende seed_version**
  - Geen expliciete “fail closed” bij unknown seed_version in consumer; SPEC zegt: consumers MUST branch on seed_version. Aanbeveling: downstream bij unknown version conservatief doen (bijv. replay weigeren of waarschuwen).

**Best practice**: Schema-stabiliteit → liever “veld altijd aanwezig, waarde null indien onbekend” dan “veld soms afwezig”; SPEC ondersteunt dat voor seeds bij early-exit.

---

## 6) Precedence/triage (mini-tabel)

| Prioriteit | Conditie | Exit | Reason (voorbeeld) |
|------------|----------|------|---------------------|
| 1 | Config/user error (trace not found, config parse, invalid args, …) | 2 | E_TRACE_NOT_FOUND, E_CFG_PARSE, E_INVALID_ARGS, … |
| 2 | Infra / judge unavailable (Error status + message heuristiek) | 3 | E_JUDGE_UNAVAILABLE, E_RATE_LIMIT, E_PROVIDER_5XX, E_TIMEOUT, E_NETWORK_ERROR |
| 3 | Judge abstain/uncertain (verdict Abstain in metrics) | 1 | E_JUDGE_UNCERTAIN |
| 4 | Test failures (Fail status) | 1 | E_TEST_FAILED |
| 5 | Strict mode violations (Warn/Flaky/Unstable) | 1 | E_POLICY_VIOLATION |
| 6 | Geen van boven | 0 | (success) |

**Bevestiging**: `decide_run_outcome` doorloopt deze volgorde; infra (2) vóór abstain (3) vóór gewone test failures (4). Geen downgrade van infra (exit 3) naar exit 1.

---

## 7) Cardinality & privacy (sanity)

- **Seeds/metrics niet als metric-labels**
  - Geen gebruik van order_seed/judge_seed of judge_metrics als labels in assay-metrics/telemetry in de codebase (grep: geen order_seed/judge_seed in assay-metrics). Ze staan alleen in run.json, summary.json en console-footer.

- **Judge metrics**: ratios/counters (abstain_rate, flip_rate, consensus_rate, unavailable_count), geen per-test identifiers in die velden.

- **Geen prompt/PII in console-footer**
  - `print_run_footer` print alleen Seeds-regel en Judge metrics-regel (getallen/strings). Prompt komt alleen voor in de per-test failure output (regel “Prompt: …” bij Fail), niet in de footer.

**Grep-voorbeelden (bewijs):**
- `order_seed|judge_seed` in `crates/assay-metrics`: geen matches.
- Console footer: alleen `print_run_footer` (Seeds + Judge metrics); prompt alleen in `print_summary` bij Fail-status.

---

## PR1 Audit Sign-Off Packet (definitief)

Bewijs voor schema-stability en contracten: drie kritieke plekken, echte output, tests, SPEC.

### 1) Diff-snippets — drie kritieke plekken

**1.1 — write_extended_run_json** (seed_version, order_seed, judge_seed altijd aanwezig; order_seed string|null, judge_seed null)

```rust
// E7.2: seeds always present; order_seed/judge_seed as string or null (SPEC: avoid JSON number precision loss)
obj.insert(
    "seed_version".to_string(),
    serde_json::json!(assay_core::report::summary::SEED_VERSION),
);
let order_seed_json = match artifacts.order_seed {
    Some(n) => serde_json::Value::String(n.to_string()),
    None => serde_json::Value::Null,
};
obj.insert("order_seed".to_string(), order_seed_json);
obj.insert("judge_seed".to_string(), serde_json::Value::Null);
```

**1.2 — write_run_json_minimal** (early-exit: seed keys altijd aanwezig)

```rust
// Minimal JSON for early exits (no artifacts available). E7.2: seed fields present for schema stability (null when unknown).
let v = serde_json::json!({
    "exit_code": outcome.exit_code,
    "reason_code": outcome.reason_code,
    "reason_code_version": assay_core::report::summary::REASON_CODE_VERSION,
    "seed_version": assay_core::report::summary::SEED_VERSION,
    "order_seed": null,
    "judge_seed": null,
    "resolution": outcome
});
```

**1.3 — Summary seeds schema-stability** (summary.json heeft altijd seeds; seeds als string|null)

- **Struct:** `pub seeds: Seeds` (niet `Option<Seeds>`), geen `skip_serializing_if` op `seeds`.
- **Seeds:** `order_seed` en `judge_seed` met `serialize_with`/`deserialize_with` → in JSON altijd key aanwezig, waarde **string** (decimal u64) of **null** (geen number, i.v.m. precision loss in JS/TS).
- **Constructors:** `Summary::success` en `Summary::failure` zetten `seeds: Seeds::default()`; `with_seeds(…)` zet alleen `order_seed`/`judge_seed`.

```rust
// summary.rs
pub seeds: Seeds,   // required, no Option

pub struct Seeds {
    pub seed_version: u32,
    #[serde(serialize_with = "serde_seed::serialize_opt_u64_as_str", deserialize_with = "serde_seed::deserialize_opt_u64_from_str")]
    pub order_seed: Option<u64>,   // JSON: "123" or null
    #[serde(serialize_with = "serde_seed::serialize_opt_u64_as_str", deserialize_with = "serde_seed::deserialize_opt_u64_from_str")]
    pub judge_seed: Option<u64>,  // JSON: string or null (null until E9)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sampling_seed: Option<u64>,
}
```

---

### 2) Echte run.json + summary.json (top-level)

**Happy path (exit 0)** — seeds als string (geen number, i.v.m. precision loss)

```json
// run.json (relevant keys)
{
  "exit_code": 0,
  "reason_code": "",
  "seed_version": 1,
  "order_seed": "17390767342376325021",
  "judge_seed": null
}

// summary.json (relevant keys)
{
  "exit_code": 0,
  "reason_code": "",
  "seeds": {
    "seed_version": 1,
    "order_seed": "17390767342376325021",
    "judge_seed": null
  }
}
```

**Early-exit (config/trace missing)**

```json
// run.json
{
  "exit_code": 2,
  "reason_code": "E_CFG_PARSE",
  "seed_version": 1,
  "order_seed": null,
  "judge_seed": null
}

// summary.json
{
  "exit_code": 2,
  "reason_code": "E_CFG_PARSE",
  "seeds": {
    "seed_version": 1,
    "order_seed": null,
    "judge_seed": null
  }
}
```

---

### 3) Tests — bewijs dat het niet regresseert

**Contract tests (crates/assay-cli/tests/contract_exit_codes.rs)**

- `assert_run_json_seeds_early_exit`: `seed_version == 1`, keys `order_seed`/`judge_seed` aanwezig, waarde null.
- `assert_run_json_seeds_happy`: `seed_version == 1`, **`order_seed` moet string zijn** (geen number, precision-safe), `judge_seed` null.
- `assert_summary_seeds_early_exit`: `seeds` met `seed_version`, keys aanwezig, `order_seed`/`judge_seed` string of null.
- `assert_summary_seeds_happy`: `seeds` met **`order_seed` string**, `judge_seed` null.
- `contract_e72_seeds_happy_path`: full run + bovenstaande assertions op run.json en summary.json.
- `contract_reason_code_trace_not_found_v2` / early-exit: minimal run.json met seed keys (null).

**Precedence test**

- `test_infra_beats_abstain_precedence` (mod.rs): één infra Error + één Abstain metric → exit 3, reason E_TIMEOUT of E_JUDGE_UNAVAILABLE.

**Golden**

- `test_golden_harness`: stderr normaliseert Seeds-regel (`order_seed=<SEED>`, `judge_seed=<JUDGE>`).

---

### 4) SPEC-consistentie — exacte regels

**SPEC-PR-Gate-Outputs-v1.md §3.3.1 (Seeds)**

- Tabel: `order_seed` en `judge_seed` type **string or null**; decimal u64 as string to avoid JSON number precision loss; judge_seed MAY be null until E9.
- Normatief (zelfde §):
  > **Normative:** run.json (extended and minimal) and summary.json SHALL include `seed_version`; order_seed and judge_seed SHALL be present (string or null). Seeds MUST be encoded as decimal strings (or null) to avoid precision loss in JSON consumers (e.g. JS/TS safe for u64 > 2^53). CLI console SHALL print one line: `Seeds: seed_version=1 order_seed=… judge_seed=…` so CI job summaries can show them for replay.

**Version history (§9)**

- Tabel regel schema_version 1:
  > Initial: … E7.2: seeds (seed_version, order_seed, judge_seed) in summary.json, run.json, and console. **judge_seed reserved; null until E9/judge sampling.** E7.3: judge_metrics …

---

## Definitive sign-off checklist (reviewer)

| Item | Bewijs in dit packet |
|------|----------------------|
| 1. write_extended_run_json: seed_version, order_seed (string|null), judge_seed null; keys altijd aanwezig | §1.1 code snippet |
| 2. write_run_json_minimal: seed keys (seed_version, order_seed: null, judge_seed: null) | §1.2 code snippet |
| 3. Summary seeds: non-optional `seeds: Seeds`; order_seed/judge_seed als string of null in JSON | §1.3 struct + serde |
| 4. Echte output: happy path (order_seed string, judge_seed null) | §2 run.json + summary.json |
| 5. Echte output: early-exit (seed keys, nulls) | §2 early-exit blok |
| 6. Contract tests: order_seed string (geen number), keys geassert; summary idem | §3 contract + precedence |
| 7. Precedence test: infra (exit 3) beats abstain (exit 1) | §3 test_infra_beats_abstain_precedence |
| 8. SPEC: seeds string or null; decimal strings to avoid precision loss; judge_seed MAY null until E9 | §4 exacte SPEC-regels |

Als deze vier blokken (diffs, output, tests, SPEC) kloppen → **✅ definitive sign-off** voor PR1 (E7.5 / E7.2 / E7.3).

---

## Checklist vóór PR open

- [ ] `Summary.seeds` is non-optional en altijd geserialiseerd.
- [ ] `Seeds.order_seed` en `Seeds.judge_seed` keys staan altijd in JSON (string of null).
- [ ] `write_extended_run_json` insert seeds altijd (geen `if let Some`); order_seed als string of null.
- [ ] Contract test faalt als `order_seed` een number is (precision-safe).
- [ ] SPEC §3.3.1: order_seed/judge_seed type string or null + normatieve zin over decimal strings.

---

## Git-diffs opnieuw genereren (optioneel)

De relevante diffs staan hierboven in §1. Om op de PR-branch zelf dezelfde diffs te zien:

```bash
git fetch origin main
git diff origin/main...HEAD -- crates/assay-cli/src/exit_codes.rs
git diff origin/main...HEAD -- crates/assay-cli/src/cli/commands/mod.rs
git diff origin/main...HEAD -- crates/assay-core/src/engine/runner.rs crates/assay-core/src/report/
git diff origin/main...HEAD -- docs/architecture/SPEC-PR-Gate-Outputs-v1.md
```

*(Lokale wijzigingen zoals `write_run_json_minimal` + `with_seeds(None, None)` bij early-exit staan mogelijk nog niet gecommit; die zijn wel in §1 als diff-snippets opgenomen.)*

---

Als je dit in de PR zet (of de drie outputs + eventuele sign-off checklist hier plakt), kan ik een concrete sign-off met “blockers” vs “nice-to-haves” geven.
