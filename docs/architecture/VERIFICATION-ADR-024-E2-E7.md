# Review Pack: ADR-024 Epics E2–E7 (CLI, Suite, Integrity, Report)

**Branch:** `feat/adr-024-e2-e6` (or equivalent)
**Epics:** E2–E7 — CLI flags, SuiteConfig, integrity attacks, report metadata, test plan
**Depends on:** E1 (VerifyLimitsOverrides) — branch `feat/adr-024-sim-hardening`
**ADR:** [ADR-024 Sim Engine Hardening](./ADR-024-Sim-Engine-Hardening.md)

---

## Review Findings & Merge Blockers (Post-Review)

### Merge Blockers

| # | Issue | Location | Risk |
|---|-------|----------|------|
| 1 | **limit_bundle_bytes alloc + cast** — `vec![0u8; (limits.max_bundle_bytes + 1) as usize]` can OOM (user-supplied 100MB+) or panic on u64→usize overflow | `integrity.rs` L135 | DoS in sim |
| 2 | **Semantics niet gegarandeerd** — Raw zeros → GzDecoder faalt op gzip header vóórdat LimitReader limit bereikt → `IntegrityTar` i.p.v. `LimitBundleBytes` | Verifier flow | Flaky regression guard |
| 3 | **Exit code mismatch** — ADR=3, impl=2 (`EXIT_CONFIG_ERROR`) | ADR vs `exit_codes.rs` | CI/UX confusion |

### Aanbevolen fixes

1. **limit_bundle_bytes**: streaming Read (geen grote alloc) + payload die LimitBundleBytes triggered (zie § Verifier Flow).
2. **Tier defaults**: één source of truth (CLI of Suite), niet beide (`sim.rs` + `suite.rs`).
3. **Exit codes**: ADR updaten of impl aanpassen.

---

## Verifier Flow: Waar wordt max_bundle_bytes afgedwongen?

**Relevant codeblocks voor `limit_bundle_bytes` + `verify_bundle_with_limits`:**

```rust
// assay-evidence/src/bundle/writer.rs L693–702
pub fn verify_bundle_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<VerifyResult> {
    let reader = EintrReader::new(reader);
    // 1. Limit INPUT size (Network protection) — RAW stream vóór gzip
    let reader = LimitReader::new(reader, limits.max_bundle_bytes, "LimitBundleBytes");
    let decoder = GzDecoder::new(reader);  // leest van LimitReader
    let limited_decoder = LimitReader::new(decoder, limits.max_decode_bytes, "LimitDecodeBytes");
    let mut archive = tar::Archive::new(limited_decoder);
    // ...
}

// LimitReader L574–607: telt bytes gelezen van inner; bij read >= limit → Err("LimitBundleBytes: exceeded...")
impl<R: Read> Read for LimitReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.read >= self.limit {
            return Err(std::io::Error::other(format!("{}: exceeded limit of {} bytes", self.error_tag, self.limit)));
        }
        let max_to_read = (self.limit - self.read).min(buf.len() as u64) as usize;
        let n = self.inner.read(&mut buf[..max_to_read])?;
        self.read += n as u64;
        Ok(n)
    }
}
```

**Flow:** `reader` → `EintrReader` → `LimitReader(max_bundle_bytes)` → `GzDecoder` → …

**Huidige attack payload:** `vec![0u8; max_bundle_bytes + 1]` (raw zeros)

- GzDecoder leest eerste bytes voor gzip header (0x1f 0x8b) → raw zeros → invalid header → fail vroeg.
- LimitReader bereikt limit niet → geen LimitBundleBytes, wel IntegrityTar / invalid gzip.

**Conclusie:** Payload moet **geldige gzip** zijn die de decoder laat doorlezen tot > limit bytes, of een streaming Read die limit+1 bytes levert zonder alloc. Optie: minimale geldige gzip stream met lengte > limit (bijv. stored deflate blocks) via streaming generator.

**Huidige attack (integrity.rs L129–136):**

```rust
run_attack(report, "integrity.limit_bundle_bytes", limits, budget,
    || Ok(vec![0u8; (limits.max_bundle_bytes + 1) as usize]))?;
```

Problemen: (1) alloc, (2) u64→usize cast, (3) raw zeros → IntegrityTar, niet LimitBundleBytes.

---

## `blocked_by` / error_code semantics

Bij `status == Blocked` wordt `error_code` gezet (bijv. `LimitBundleBytes`, `IntegrityTar`). Dit fungeert als `blocked_by`. Zorg dat bij Bypassed/Error de semantics duidelijk zijn; overweeg expliciete `blocked_by` field i.p.v. overloaded `error_code`.

---

## Review Checklist

### E2: CLI Flags & Parsing

| Criterion | Location | Verify |
|----------|----------|--------|
| `--limits` (JSON string or `@path`) | `args.rs` L1176–1178 | Present on `SimRunArgs` |
| `--limits-file` | `args.rs` L1180–1182 | Present on `SimRunArgs` |
| `--time-budget` (default 60) | `args.rs` L1184–1186 | `default_value = "60"` |
| `--print-config` | `args.rs` L1188–1190 | Boolean flag |
| `--limits @path` parsing | `sim.rs` L18–23 | `s.starts_with('@')` → load from file |
| Merge precedence: tier → limits → limits_file | `sim.rs` L16–36 | `parse_limits` applies in order |
| Config errors → exit | `sim.rs` L50–73 | `EXIT_CONFIG_ERROR` on invalid JSON, missing file, time_budget ≤0 |

### E3: SuiteConfig & TimeBudget

| Criterion | Location | Verify |
|----------|----------|--------|
| `time_budget_secs` on SuiteConfig | `suite.rs` L23–25 | Passed from CLI |
| `TimeBudget::new(Duration::from_secs(cfg.time_budget_secs))` | `suite.rs` L75 | Configurable, not hardcoded 60 |
| Tier-default limits: Quick 5MB | `sim.rs` L40–46, `suite.rs` L37–43 | `max_bundle_bytes = 5 * 1024 * 1024` |
| `verify_limits` passed to integrity | `suite.rs` L76–78, L90–95 | `limits` from config or tier default |

### E4: Integrity Attacks (Limits + Budget)

| Criterion | Location | Verify |
|----------|----------|--------|
| `verify_bundle_with_limits(cursor, limits)` | `integrity.rs` L173 | Not `verify_bundle` |
| Budget check before each attack | `integrity.rs` L167–169 | `if budget.exceeded() { Err(BudgetExceeded) }` |
| `IntegrityError::BudgetExceeded` / `Other` | `integrity.rs` L147–156 | Handled in suite.rs L101–128 |
| `blocked_by` in results | `report.rs` | `error_code` populated when Blocked (e.g. `IntegrityTar`, `LimitBundleBytes`) |

### E5: Dynamic limit_bundle_bytes Attack

| Criterion | Location | Verify |
|----------|----------|--------|
| Attack name `integrity.limit_bundle_bytes` | `integrity.rs` L132–135 | Present |
| Payload size = `limits.max_bundle_bytes + 1` | `integrity.rs` L135 | `vec![0u8; (limits.max_bundle_bytes + 1) as usize]` |
| Compressed-size target | ADR | Moet geldige gzip zijn; raw bytes → IntegrityTar (zie § Verifier Flow) |

### E6: Report Metadata & Budget UX

| Criterion | Location | Verify |
|----------|----------|--------|
| `time_budget_exceeded: bool` | `report.rs` L11–13 | On `SimReport` |
| `skipped_phases: Vec<String>` | `report.rs` L14–16 | e.g. `["differential", "chaos"]` |
| `set_time_budget_exceeded(skipped)` | `report.rs` L60–63 | Called when budget exceeded |
| Budget-exceeded message in CLI | `sim.rs` L112–116 | "⏱ Time budget exceeded. Skipped: ..." |
| Exit 2 when `report.time_budget_exceeded` | `sim.rs` L110–117 | `return Ok(2)` |

### E7: --print-config & Test Plan

| Criterion | Location | Verify |
|----------|----------|--------|
| `--print-config` prints limits + time_budget | `sim.rs` L76–84 | Early return after println |
| Output: max_bundle_bytes, max_decode_bytes, time_budget | `sim.rs` L79–81 | Keys present |

---

## Review Nits (niet-blockers)

| Epic | Nit | Aanbeveling |
|------|-----|-------------|
| E2 | Double-source (`@path` + `--limits-file`) | Optional: log/print welk pad effectief in verbose |
| E2 | Unknown keys error | Expliciet mappen serde error → "unknown field" voor betere UX |
| E3 | Tier defaults dubbele bron | Unify: CLI als source of truth, suite gebruikt alleen config |
| E4 | Budget-check na verify | ADR: "after each expensive verify"; nu alleen vóór mutator |
| E6 | elapsed/remaining in budget UX | ADR vraagt "time consumed / remaining" — toevoegen indien TimeBudget dit levert |
| E7 | `--print-config` machine-readable | Overweeg `--print-config=json` voor CI/tooling |

---

## Test Plan (ADR § Test Plan)

| # | Command | Expected |
|---|---------|----------|
| 1 | `assay sim run --suite quick --target bundle.tar.gz --limits '{"max_bundle_bytes": 1000}'` | zip_bomb and limit_bundle_bytes blocked |
| 2 | `assay sim run --suite quick --target bundle.tar.gz --limits-file /nonexistent` | Exit 2, "Config error: limits file not found" |
| 3 | `assay sim run --suite quick --target bundle.tar.gz --limits 'invalid'` | Exit 2, "Config error: invalid --limits JSON" |
| 4 | `assay sim run --suite quick --target bundle.tar.gz --limits '{"max_bundle_bytess": 1}'` | Exit 2, error contains "unknown" or serde reject |
| 5 | `assay sim run ... --limits '{"max_bundle_bytes": 1000}' --limits-file .assay/stricter.json` | File wins over --limits (merge precedence) |
| 6 | `assay sim run --suite quick --target bundle.tar.gz --time-budget 0` | Exit 2, "Config error: --time-budget must be > 0" |
| 7 | `assay sim run --suite quick --target bundle.tar.gz --time-budget 1` | Exit 2, output contains "Time budget exceeded" and "Skipped:" |
| 8 | `assay sim run --suite quick --target bundle.tar.gz --print-config` | Output includes max_bundle_bytes, max_decode_bytes, time_budget |
| 9 | `assay sim run --suite quick --target bundle.tar.gz` (default) | All attacks blocked; limit_bundle_bytes present; Quick tier ~5MB |

**Note:** ADR specifies exit 3 for config errors; implementation uses `EXIT_CONFIG_ERROR = 2` (workspace convention). Test plan hieronder gebruikt 2; align ADR of impl.

---

## Verification Commands

```bash
# Build
cargo build -p assay-sim -p assay-cli

# Unit tests
cargo test -p assay-sim --lib
cargo test -p assay-evidence --lib

# Lint
cargo clippy -p assay-sim -p assay-cli --all-targets -- -D warnings

# E2E (test bundle required)
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz

# Print config
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --print-config

# Strict limits (test plan #1)
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --limits '{"max_bundle_bytes": 1000}'

# Config errors (test plan #2, #6)
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --limits-file /nonexistent
cargo run -p assay-cli -- sim run --suite quick --target tests/fixtures/evidence/test-bundle.tar.gz --time-budget 0
```

---

## Key Code Snippets

### sim.rs: parse_limits + @path

```rust
/// Parse limits from CLI. Merge precedence: tier default → --limits → --limits-file.
fn parse_limits(args: &SimRunArgs) -> Result<VerifyLimits> {
    let mut defaults = tier_default_limits(args.suite.to_lowercase().as_str());
    if let Some(ref s) = args.limits {
        let overrides = if s.starts_with('@') {
            let path = s.trim_start_matches('@').trim();
            let content = fs::read_to_string(path)
                .with_context(|| format!("limits file not found: {}", path))?;
            serde_json::from_str::<VerifyLimitsOverrides>(&content)
                .with_context(|| format!("invalid limits JSON in {}", path))?
        } else {
            serde_json::from_str::<VerifyLimitsOverrides>(s)
                .context("invalid --limits JSON (use --limits-file or --limits @path for file)")?
        };
        defaults = defaults.apply(overrides);
    }
    if let Some(ref p) = args.limits_file {
        // ... apply overrides from file
    }
    Ok(defaults)
}
```

### integrity.rs: run_attack + budget + verify_bundle_with_limits

```rust
fn run_attack<F>(..., limits: VerifyLimits, budget: &TimeBudget, mutator: F) -> Result<(), IntegrityError>
where F: FnOnce() -> AnyhowResult<Vec<u8>>,
{
    if budget.exceeded() {
        return Err(IntegrityError::BudgetExceeded);
    }
    let data = mutator()?;
    let res = verify_bundle_with_limits(Cursor::new(data), limits);
    // ... report Blocked with error_code (blocked_by)
    Ok(())
}

// limit_bundle_bytes attack
run_attack(report, "integrity.limit_bundle_bytes", limits, budget,
    || Ok(vec![0u8; (limits.max_bundle_bytes + 1) as usize]))?;
```

### suite.rs: IntegrityError handling

```rust
match attacks::integrity::check_integrity_attacks(&mut inner_report, seed, limits, &budget) {
    Ok(()) => { /* merge results */ }
    Err(IntegrityError::BudgetExceeded) => {
        report.set_time_budget_exceeded(vec!["differential".into(), "chaos".into()]);
        report.add_result(AttackResult { name: "time_budget".into(), status: Error, ... });
        return Ok(report);
    }
    Err(IntegrityError::Other(e)) => { /* add integrity_attacks error result */ }
}
```

---

## ADR Alignment

| ADR § | Requirement | Status |
|-------|-------------|--------|
| CLI §1 | --limits, --limits-file, --time-budget, --print-config | ✓ |
| Merge precedence | tier default → --limits → --limits-file | ✓ |
| @path | --limits value starts with @ → load from file | ✓ |
| Limits model | VerifyLimitsOverrides + apply | ✓ (E1) |
| Integrity attacks | verify_bundle_with_limits, budget check | ✓ |
| limit_bundle_bytes | Compressed size = limit + 1 | ✓ |
| Tier defaults | Quick 5MB | ✓ |
| Exit codes | Time budget exceeded → 2; config → 2* | ✓ (*ADR says 3) |
| Report metadata | time_budget_exceeded, skipped_phases | ✓ |
| blocked_by | error_code in AttackResult when Blocked | ✓ |

---

## Merge Gates

### Pre-merge (blockers)

- [x] **limit_bundle_bytes**: streaming/no huge alloc + deterministic LimitBundleBytes (niet IntegrityTar)
- [x] **Tier defaults**: één source of truth (`tier_default_limits` in assay-sim, gebruikt door CLI + suite)
- [x] **Exit codes**: ADR geüpdatet naar exit 2 (workspace convention)

### Standard

- [ ] `cargo build -p assay-sim -p assay-cli` passes
- [ ] `cargo test -p assay-sim --lib` passes (incl. `test_quick_suite`)
- [ ] `cargo test -p assay-evidence --lib` passes
- [ ] `cargo clippy -p assay-sim -p assay-cli --all-targets -- -D warnings` passes
- [ ] Test plan items 1, 2, 6, 8, 9 executed and pass
- [ ] **Regression test**: limit_bundle_bytes → `error_code == LimitBundleBytes` (niet IntegrityTar)
- [ ] Branch depends on E1 (VerifyLimitsOverrides) — merge E1 first or rebase

---

## Acceptance

- [ ] Merge blockers opgelost
- [ ] All checklist items pass
- [ ] Test plan scenarios verified
- [ ] ADR-024 Epics table: E2–E7 marked implemented
- [ ] Exit code convention (2 vs 3 for config) documented or aligned
