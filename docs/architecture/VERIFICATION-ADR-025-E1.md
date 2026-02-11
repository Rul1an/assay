# Review Pack: ADR-025 Epic 1 (Soak MVP)

**Branch:** `feat/adr-025-epics` (or equivalent)
**Epic:** E1 — Policy soak testing: N runs, pass^k semantics, soak-report-v1, exit codes 0/1/2
**Depends on:** Pack engine + lint (assay-evidence); cicd-starter default pack (ADR-023)
**ADR:** [ADR-025 Evidence-as-a-Product](./ADR-025-Evidence-as-a-Product.md) | [Epics](./ADR-025-Epics.md)

---

## Review Checklist

### Functional

| Criterion | Location | Verify |
|-----------|----------|--------|
| `assay sim soak` subcommand | `args.rs` SimSub::Soak | `Soak(SoakArgs)` |
| `--iterations N` (default 10) | `args.rs` SoakArgs | `#[arg(long, default_value = "10")]` |
| `--target <bundle>` (required) | `args.rs` SoakArgs | `pub target: PathBuf` |
| `--pack` (default: cicd-starter) | `sim.rs` L199–201 | Same as lint; `load_packs` |
| `--decision-policy error\|warning\|info` | `args.rs`, `sim.rs` L179–190 | Severity mapping; exit 2 on invalid |
| `--report <path>` or `-` | `args.rs` SoakArgs | `pub report: Option<PathBuf>`; `-` → stdout |
| `--seed`, `--time-budget` | `args.rs` SoakArgs | Optional seed; time_budget default 300 |
| Report: schema_version, pass_rate, pass_all | `soak.rs` SoakReport | `soak-report-v1` |
| Report: packs[] (name, version, digest, kind) | `soak.rs` PackRef, `sim.rs` L308–317 | Populated from LoadedPack |
| Report: violations_by_rule canonical `pack@ver:rule` | `sim.rs` L259–261 | From LintFinding.rule_id (already canonical) |
| Pack loader = lint | `sim.rs` L204, L216 | `load_packs` + `lint_bundle_with_options` |

### Exit Codes

| Code | Betekenis | Trigger |
|------|-----------|---------|
| 0 | All pass (pass_all) | No findings ≥ threshold |
| 1 | ≥1 policy fail | `failures > 0` |
| 2 | Infra error | `infra_errors > 0` (time budget, verification failed) |
| 2 | Config error | `--iterations 0`, invalid `--decision-policy` |
| 3 | Pack loading failed | `load_packs` error |

### UX / Human Summary

| Element | Expected |
|---------|----------|
| pass_rate | "Pass rate: X.X%" |
| first_policy_failure_at | "First policy failure at run: N" (if any) |
| first_infra_error_at | "First infra error at run: N" (if any) |
| top 3 violated rules | "Top violated rules: [(rule_id, count), ...]" |
| Infra error message | "❌ Infra error: N runs failed verification" |
| Policy fail message | "❌ Policy fail: N runs had findings ≥ {severity}" |

---

## Report Schema (soak-report-v1)

| Field | Required | Type | Notes |
|-------|----------|------|-------|
| schema_version | ✓ | string | `"soak-report-v1"` |
| mode | ✓ | string | `"soak"` |
| variation_source | ✓ | string | `"deterministic_repeat"` (MVP) |
| time_budget_scope | ✓ | string | `"soak"` (global) |
| iterations | ✓ | integer | N runs |
| seed | ✓ | integer | Reproducibility |
| time_budget_secs | ✓ | integer | |
| limits | ✓ | object | max_bundle_bytes, max_decode_bytes, etc. |
| packs | ✓ | array | name, version, digest (required), kind?, source? |
| decision_policy | ✓ | object | pass_on_severity_at_or_above |
| results | ✓ | object | runs, passes, failures, infra_errors, pass_rate, pass_all |
| results.first_policy_failure_at | | integer\|null | 1-based |
| results.first_infra_error_at | | integer\|null | 1-based |
| results.violations_by_rule | | object | canonical rule id → count |
| runs | | array | Per-run RunResult (index, status, duration_ms, violated_rules?) |

---

## Key Code Snippets

### args.rs: SoakArgs

```rust
#[derive(clap::Args, Clone, Debug)]
pub struct SoakArgs {
    #[arg(long, default_value = "10")]
    pub iterations: u32,

    #[arg(long, short)]
    pub target: std::path::PathBuf,

    #[arg(long, value_delimiter = ',')]
    pub pack: Option<Vec<String>>,

    #[arg(long, default_value = "error", value_name = "SEVERITY")]
    pub decision_policy: String,

    #[arg(long)]
    pub seed: Option<u64>,

    #[arg(long, default_value = "300")]
    pub time_budget: u64,

    #[arg(long)]
    pub report: Option<std::path::PathBuf>,
}
```

### sim.rs: Exit code precedence

```rust
if infra_errors > 0 {
    eprintln!("\n❌ Infra error: {} runs failed verification", infra_errors);
    return Ok(2);
}
if failures > 0 {
    eprintln!("❌ Policy fail: {} runs had findings ≥ {}", failures, args.decision_policy);
    return Ok(1);
}
println!("\n✅ All {} runs passed.", args.iterations);
Ok(0)
```

### sim.rs: --report - (stdout)

```rust
if let Some(ref path) = args.report {
    let json = serde_json::to_string_pretty(&report)?;
    if path.as_os_str() == "-" {
        print!("{}", json);
    } else {
        std::fs::write(path, json)?;
    }
}
```

---

## Test Plan

| # | Command | Expected |
|---|---------|----------|
| 1 | `assay sim soak --target tests/fixtures/evidence/test-bundle.tar.gz --iterations 3` | Exit 0; pass_rate 100%; human summary |
| 2 | `assay sim soak --target ... --iterations 3 --report soak.json` | Report file created; JSON valid |
| 3 | `assay sim soak --target ... --report -` | JSON to stdout; exit 0 |
| 4 | `assay sim soak --target ... --decision-policy info` | Exit 1 (info findings); violations_by_rule has cicd-starter@1.0.0:CICD-004 |
| 5 | `assay sim soak --target ... --pack cicd-starter` | Same as default |
| 6 | `assay sim soak --target /nonexistent --iterations 1` | Error "failed to read bundle" |
| 7 | `assay sim soak --target ... --iterations 0` | Exit 2, "Config error: --iterations must be > 0" |
| 8 | `assay sim soak --target ... --decision-policy foo` | Exit 2, "Config error: --decision-policy must be error, warning, or info" |
| 9 | `assay sim soak --target ... --time-budget 1` | Exit 2 if >1 iteration and slow; infra_errors_by_kind has time_budget_exceeded |

---

## Verification Commands

```bash
# Build
cargo build -p assay-cli --features sim

# Happy path (compliant bundle, default decision-policy error)
cargo run -p assay-cli -- sim soak --target tests/fixtures/evidence/test-bundle.tar.gz --iterations 3 --report -
# → exit 0, JSON with schema_version soak-report-v1, pass_all: true

# Policy fail (decision-policy info → CICD-004 info finding fails)
cargo run -p assay-cli -- sim soak --target tests/fixtures/evidence/test-bundle.tar.gz --iterations 2 --decision-policy info
# → exit 1, "Policy fail: 2 runs had findings ≥ info", violations_by_rule cicd-starter@1.0.0:CICD-004

# Config errors
cargo run -p assay-cli -- sim soak --target tests/fixtures/evidence/test-bundle.tar.gz --iterations 0
# → exit 2

cargo run -p assay-cli -- sim soak --target tests/fixtures/evidence/test-bundle.tar.gz --decision-policy invalid
# → exit 2

# Schema test
cargo test -p assay-sim soak_report_schema
```

---

## ADR Alignment

| ADR § | Requirement | Status |
|-------|-------------|--------|
| E1 CLI | --iterations, --pack, --target, --report | ✓ |
| E1 Report | schema_version, pass_rate, pass_all, decision_policy, packs[] | ✓ |
| E1 violations_by_rule | Canonical pack@ver:rule_id | ✓ (from lint) |
| E1 --decision-policy | CLI flag; error/warning/info | ✓ |
| E1 Human summary | pass_rate, first_policy_failure_at, top 3 rules (sorted) | ✓ |
| E1 --report - | Stdout for CI piping | ✓ |
| E1 Pack loader | Same as lint (normative) | ✓ |
| E1 Exit codes | 0 pass, 1 policy, 2 infra | ✓ |
| Variatiebron | Optie B (MVP): N× same bundle; Optie A deferred | ⚠ Scope: stability pipeline, not multi-run variance |

---

## Review Nits (niet-blockers)

| Nit | Aanbeveling |
|-----|-------------|
| Variatiebron | `variation_source: "deterministic_repeat"` in report; Optie A deferred |
| --limits | Soak uses VerifyLimits::default(); no --limits/--limits-file yet |
| run_results size | Always populated; for large N consider --no-runs or truncate |
| Pack loading exit 3 | Align with lint (exit 3); document in exit_codes |

---

## Merge Gates

- [ ] Soak report schema validation test passes
- [ ] Canonical rule-id: lint and soak use same format (pack@ver:rule)
- [ ] packs[], limits, seed always in report
- [ ] Test plan items 1, 2, 4, 7, 8 executed
- [ ] `cargo clippy -p assay-cli --features sim -- -D warnings` passes

---

## Acceptance

- [ ] All E1 acceptance criteria verified
- [ ] Test plan scenarios executed
- [ ] ADR-025 Epics: E1 marked implemented
