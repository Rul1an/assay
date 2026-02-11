# Review Pack: ADR-024 Epic 6 (Report Metadata & Budget UX)

**Branch:** `feat/adr-024-e2-e6` (or equivalent)
**Epic:** E6 — Report metadata: `time_budget_exceeded`, `blocked_by`, `phase`, `skipped_phases`; budget-exceeded UX
**Depends on:** E4 (Integrity attacks + budget check)
**ADR:** [ADR-024 Sim Engine Hardening](./ADR-024-Sim-Engine-Hardening.md)

---

## Review Checklist

### SimReport Metadata

| Criterion | Location | Verify |
|-----------|----------|--------|
| `time_budget_exceeded: bool` on SimReport | `report.rs` L11–13 | Present; `#[serde(skip_serializing_if = "Not::not")]` |
| `skipped_phases: Vec<String>` on SimReport | `report.rs` L14–16 | Present; `#[serde(skip_serializing_if = "Vec::is_empty")]` |
| `set_time_budget_exceeded(skipped)` | `report.rs` L59–63 | Sets both fields |
| `blocked_by` semantics | `report.rs` L77–78 | `error_code` populated when `AttackStatus::Blocked` |

### Budget-Exceeded Flow

| Criterion | Location | Verify |
|-----------|----------|--------|
| Budget exceeded during integrity phase | `suite.rs` L101–115 | `IntegrityError::BudgetExceeded` → `set_time_budget_exceeded(["differential","chaos"])` |
| Budget exceeded after integrity | `suite.rs` L132–143 | `set_time_budget_exceeded(["differential","chaos"])` |
| Budget exceeded after differential | `suite.rs` L159–170 | `set_time_budget_exceeded(["chaos"])` |
| Budget exceeded during chaos | `suite.rs` L202–212 | No `set_time_budget_exceeded` (chaos is last) |

### CLI UX

| Criterion | Location | Verify |
|-----------|----------|--------|
| Budget-exceeded message in CLI | `sim.rs` L112–117 | "⏱ Time budget exceeded. Skipped: …" |
| Exit 2 when `report.time_budget_exceeded` | `sim.rs` L110–118 | `return Ok(2)` before results table |

### Machine-Readable Contract (ADR §5)

| Field | Expected | Status |
|-------|----------|--------|
| `blocked_by` | error code when Blocked | ✓ Via `error_code` (same semantics) |
| `phase` | integrity \| differential \| chaos | ⚠ Implicit in `name` (e.g. "integrity.bitflip"); geen expliciet veld |
| `skipped_phases` | array when budget exceeded | ✓ |
| `time_budget_exceeded` | boolean | ✓ |

---

## Budget-Exceeded Flow Diagram

```
run_suite()
    │
    ├─ [1] Integrity phase
    │     └─ BudgetExceeded? → set_time_budget_exceeded(["differential","chaos"])
    │                         add_result("time_budget", Error, "during integrity phase")
    │                         return Ok(report)
    │
    ├─ budget.exceeded() after integrity? → set_time_budget_exceeded(["differential","chaos"])
    │                                     add_result("time_budget", Error, "after integrity phase")
    │                                     return Ok(report)
    │
    ├─ [2] Differential phase
    │
    ├─ budget.exceeded() after differential? → set_time_budget_exceeded(["chaos"])
    │                                        add_result("time_budget", Error, "after differential phase")
    │                                        return Ok(report)
    │
    └─ [3] Chaos phase (if tier == Chaos)
          └─ budget.exceeded() during chaos? → add_result("time_budget", Error, "during chaos phase")
                                              (geen set_time_budget_exceeded — chaos is laatste)
```

**Nit:** Na chaos wordt `set_time_budget_exceeded` niet aangeroepen — er is geen volgende phase om te skippen. Het `time_budget` result wordt wel toegevoegd. Voor consistentie: als budget exceeded tijdens chaos, zou `skipped_phases = []` of niet-gezet kunnen zijn. Huidige impl: chaos phase voegt alleen `add_result` toe, geen `set_time_budget_exceeded`. Dan blijft `report.time_budget_exceeded == false` en `skipped_phases == []`. **Bug:** CLI checks `report.time_budget_exceeded` voor exit 2; die is dan false → exit 0. Moet gefixt: ook bij chaos budget exceeded `set_time_budget_exceeded([])` aanroepen zodat exit 2 correct is.

---

## Key Code Snippets

### report.rs: SimReport + set_time_budget_exceeded

```rust
#[derive(Debug, Serialize, Clone)]
pub struct SimReport {
    pub suite: String,
    pub seed: u64,
    pub summary: SimSummary,
    pub results: Vec<AttackResult>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub time_budget_exceeded: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skipped_phases: Vec<String>,
}

impl SimReport {
    pub fn set_time_budget_exceeded(&mut self, skipped: Vec<String>) {
        self.time_budget_exceeded = true;
        self.skipped_phases = skipped;
    }
    // ...
}
```

### report.rs: error_code als blocked_by

```rust
Ok((class, code)) => {
    self.summary.blocked += 1;
    AttackResult {
        name: name.to_string(),
        status: AttackStatus::Blocked,
        error_class: Some(format!("{:?}", class)),
        error_code: Some(format!("{:?}", code)),  // ← blocked_by
        message: None,
        duration_ms,
    }
}
```

### sim.rs: CLI exit 2 + message

```rust
if report.time_budget_exceeded {
    eprintln!(
        "\n⏱ Time budget exceeded. Skipped: {}",
        report.skipped_phases.join(", ")
    );
    return Ok(2);
}
```

### suite.rs: BudgetExceeded handling

```rust
Err(attacks::integrity::IntegrityError::BudgetExceeded) => {
    for r in inner_report.results { report.add_result(r); }
    report.set_time_budget_exceeded(vec!["differential".into(), "chaos".into()]);
    report.add_result(AttackResult {
        name: "time_budget".into(),
        status: AttackStatus::Error,
        message: Some("time budget exceeded during integrity phase".into()),
        duration_ms: budget.elapsed().as_millis() as u64,
        ..
    });
    return Ok(report);
}
```

---

## Test Plan (E6-specifiek)

| # | Command | Expected |
|---|---------|----------|
| 1 | `assay sim run --suite quick --target bundle.tar.gz --time-budget 1` | Exit 2; output "⏱ Time budget exceeded. Skipped: …" |
| 2 | `assay sim run ... --time-budget 1 --report out.json` | `out.json` has `time_budget_exceeded: true`, `skipped_phases` non-empty |
| 3 | Normal run (budget OK) | `time_budget_exceeded: false`, `skipped_phases: []` in JSON |
| 4 | Blocked attack result | `error_code` populated (e.g. "LimitBundleBytes") when status Blocked |

---

## Verification Commands

```bash
# Time budget 1s → expect exit 2, budget exceeded
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --time-budget 1

# JSON report with budget exceeded
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz \
  --time-budget 1 --report /tmp/sim.json
cat /tmp/sim.json | jq '.time_budget_exceeded, .skipped_phases'
# → true, ["differential", "chaos"] (of ["chaos"] afhankelijk van timing)
```

---

## ADR Alignment

| ADR § | Requirement | Status |
|-------|-------------|--------|
| Machine-readable §5 | `blocked_by` | ✓ Via `error_code` |
| Machine-readable §5 | `phase` | ⚠ Implicit in name; geen expliciet veld |
| Machine-readable §5 | `skipped_phases` | ✓ |
| Machine-readable §5 | `time_budget_exceeded` | ✓ |
| Budget-exceeded output §6 | "which phases were skipped" | ✓ |
| Budget-exceeded output §6 | "time consumed / remaining" | ⚠ Niet geïmplementeerd (nit) |
| Exit 2 when budget exceeded | ✓ |

---

## Review Nits & Potentiële Issues

| Item | Beschrijving | Aanbeveling |
|------|--------------|-------------|
| Chaos phase budget exceeded | ~~`set_time_budget_exceeded` niet aangeroepen~~ | ✅ Fixed: `set_time_budget_exceeded([])` toegevoegd in `run_chaos_phase` |
| elapsed/remaining in message | ADR §6: "time consumed / remaining" | Toevoegen indien TimeBudget dit levert (TimeBudget heeft `elapsed()`, `remaining()`) |
| `phase` field | ADR vraagt expliciet `phase` | Optioneel: toevoegen aan AttackResult of documenteer dat `name` prefix (integrity., differential., chaos.) phase encodeert |
| skipped_phases consistentie | Na integrity: ["differential","chaos"]; na differential: ["chaos"]; na chaos: ? | Chaos: skipped_phases = [] (geen volgende phase) |

---

## Merge Gates

- [x] `time_budget_exceeded` + `skipped_phases` correct gezet bij alle exit-paden
- [x] Chaos phase: `set_time_budget_exceeded([])` bij budget exceeded (zodat exit 2 correct)
- [ ] Test plan item 1, 2, 3 uitgevoerd
- [ ] `cargo test -p assay-sim --lib` passes (incl. test_quick_suite)

---

## Acceptance

- [ ] Alle checklist items pass
- [ ] Budget-exceeded UX correct (message + exit 2)
- [ ] JSON report heeft `time_budget_exceeded` en `skipped_phases`
- [ ] ADR-024 Epics: E6 marked implemented
