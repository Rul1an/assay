# Review Pack: ADR-025 Epic E1.1 (Soak Two-Mode)

**Branch:** `feat/adr-025-epics` (or equivalent)
**Epic:** E1.1 — Two-mode soak: artifact vs run, honest UX, run-mode stub
**Depends on:** E1 Soak MVP (VERIFICATION-ADR-025-E1)
**ADR:** [ADR-025-E1.1 Soak Two-Mode](./ADR-025-E1.1-Soak-Two-Mode.md)

---

## Review Checklist

### Functional

| Criterion | Location | Verify |
|-----------|----------|--------|
| `--mode artifact\|run` (default artifact) | `args.rs` SoakMode, SoakArgs | ValueEnum; `#[default]` artifact |
| `--mode=run` → exit config error | `sim.rs` cmd_soak | "run-mode not yet implemented" |
| `--target` required when mode=artifact | args.rs, sim.rs | `required_if_eq("mode","artifact")` + runtime fallback |
| `--run-cmd` required when mode=run | args.rs | `required_if_eq("mode","run")`; clap enforces |
| `--target` conflicts with `--run-cmd` | args.rs | `conflicts_with = "run_cmd"` on target; `conflicts_with = "target"` on run_cmd |
| `--run-cmd` in args | args.rs SoakArgs | `pub run_cmd: Option<String>` |
| `--mode=artifact` + `--run-cmd` → clap error | args.rs | conflicts_with; "cannot be used with" |
| `--mode=run` + `--target` → clap error | args.rs | conflicts_with |
| `--quiet` suppresses artifact warning | args.rs, sim.rs | `pub quiet: bool`; no warning when set |
| Report: `soak_mode` field | `soak.rs` SoakReport | "artifact" \| "run" |
| `variation_source` artifact | sim.rs | `"deterministic_repeat"` |
| `variation_source` run (unreachable) | sim.rs | `"run_trajectories"` (in match arm) |

### UX / Honest Positioning

| Element | Expected |
|---------|----------|
| artifact-mode warning | "Note: --mode=artifact repeats lint on a fixed bundle. This measures policy determinism/report stability, not agent variance/drift. Use --mode=run --run-cmd <cmd> for pass^k under variance." |
| run-mode stub message | "--mode=run is not implemented yet (E1.1). This command will run your agent N times... Track: ADR-025 E1.2." |
| CLI help mode artifact | "N× lint on same bundle — policy determinism, report stability (not drift)" |
| CLI help mode run | "N× run cmd → new bundle → lint — true variance/drift (E1.1+)" |
| Human output | Mode, Variation, Threshold, Seed |

### Backwards Compatibility

| Scenario | Expected |
|----------|----------|
| `assay sim soak --target bundle.tar.gz` | Works; default mode=artifact |
| `assay sim soak --target ... --iterations 5` | Same as E1 behavior |
| Report JSON | New field `soak_mode`; existing fields unchanged |

---

## Exit Codes (unchanged from E1)

| Code | Betekenis | Trigger |
|------|-----------|---------|
| 0 | All pass | No findings ≥ threshold |
| 1 | Policy fail | `failures > 0` |
| 2 | Infra / config | `infra_errors > 0`, `--iterations 0`, run-mode stub, missing --target |
| 3 | Pack loading failed | `load_packs` error |

---

## Report Schema Extension (soak-report-v1)

| Field | Required | Type | Notes |
|-------|----------|------|-------|
| soak_mode | ✓ | string | `"artifact"` \| `"run"` (new in E1.1) |
| mode | ✓ | string | `"soak"` (unchanged) |
| variation_source | ✓ | string | `"deterministic_repeat"` (artifact) |
| seed_strategy | | string | `"fixed"` (artifact, E1.2 forward-compat) |

---

## Test Plan

| # | Command | Expected |
|---|---------|----------|
| 1 | `assay sim soak --mode=run` | Exit 2; clap: "required: --run-cmd" |
| 1b | `assay sim soak --mode=run --run-cmd "echo noop"` | Exit 2; "not implemented yet (E1.1)"; "ADR-025 E1.2" |
| 2 | `assay sim soak --iterations 3` (no --target) | Exit 2; "--target is required when --mode=artifact" |
| 3 | `assay sim soak --target ... --iterations 3` | Exit 0; sharper warning; JSON has soak_mode: "artifact" |
| 3b | `assay sim soak --target ... --quiet` | No warning in stderr |
| 3c | `assay sim soak --mode=artifact --target ... --run-cmd X` | Clap error: "cannot be used with" |
| 4 | `assay sim soak --target ... --mode=artifact --report -` | JSON to stdout; soak_mode present |
| 5 | `assay sim soak --target ... --iterations 1 --report -` | Same bundle behavior; no regression |
| 6 | `assay sim soak --help` | Shows --mode, artifact/run descriptions |

---

## Verification Commands

```bash
# Build
cargo build -p assay-cli --features sim

# Run-mode: clap requires run-cmd
cargo run -p assay-cli -- sim soak --mode=run
# → clap: "required: --run-cmd"

# Run-mode stub (hits "not implemented" branch)
cargo run -p assay-cli -- sim soak --mode=run --run-cmd "echo noop"
# → exit 2, "not implemented yet (E1.1)... ADR-025 E1.2"

# Missing target (artifact default)
cargo run -p assay-cli -- sim soak --iterations 3
# → exit 2, "--target is required when --mode=artifact"

# Artifact mode (happy path + warning)
cargo run -p assay-cli -- sim soak --target tests/fixtures/evidence/test-bundle.tar.gz --iterations 3 --report -
# → exit 0; stderr has "Note: fixed bundle"; JSON has soak_mode: "artifact"

# Schema test (includes soak_mode, seed_strategy)
cargo test -p assay-sim soak_report_schema

# E1.1 integration tests (assert_contains pattern; no brittle output matching)
cargo test -p assay-cli --test contract_soak_e1_1

# Clippy
cargo clippy -p assay-cli -p assay-sim -- -D warnings
```

---

## ADR Alignment

| ADR § | Requirement | Status |
|-------|-------------|--------|
| Phase 1.1 | --mode artifact\|run | ✓ |
| Phase 1.2 | run stub: "not yet implemented" | ✓ |
| Phase 1.2 | UX warning in artifact-mode | ✓ |
| Phase 1.3 | Report soak_mode | ✓ |
| Phase 1.4 | Backwards compat (no --mode = artifact) | ✓ |
| Phase 1.4 | --target required artifact | ✓ |

---

## Validation Notes (review checks A/B)

- **ValueEnum:** artifact/run (lowercase). `--mode Artifact` fails; clap suggests "artifact".
- **Exit codes:** clap usage error and runtime config error both exit 2. Tests tolerate 1|2 for portability.
- **variation_source:** consts in assay-sim (`VAR_SRC_DETERMINISTIC_REPEAT`, `VAR_SRC_RUN_TRAJECTORIES`).

---

## Merge Gates

- [x] Clap: required_if_eq for target (artifact) and run_cmd (run)
- [x] Clap: conflicts_with between target and run_cmd
- [x] Run-mode stub exits with E1.2 track message
- [x] Artifact mode prints sharper warning (not drift/variance, use run-cmd)
- [x] --quiet suppresses artifact warning
- [x] Report contains soak_mode, variation_source, seed_strategy
- [x] Human output: Mode, Variation, Threshold
- [x] Integration tests: soak_mode_run_requires_run_cmd, stub_hits_runtime, artifact_requires_target, target_conflicts_run_cmd, artifact_quiet_json_contract
- [x] `cargo clippy -p assay-cli -p assay-sim -- -D warnings` passes

---

## Acceptance

- [ ] All E1.1 acceptance criteria verified
- [ ] Test plan scenarios executed
- [ ] ADR-025-E1.1 Phase 1 checklist complete
