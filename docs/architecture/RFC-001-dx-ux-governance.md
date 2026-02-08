# RFC-001: DX/UX & Governance - Core Invariants + Debt-Ranked Execution Plan

> **Status**: Active (Wave A merged, Wave B in progress)
> **Date**: 2026-02-07
> **Owner**: DX/Governance track
> **Motivation**: Keep Assay's state-of-the-art core (replay/evidence/enforcement) strong while preventing CLI/plumbing debt from eroding the product wedge.
> **Constraint**: Refactor only where it directly reduces wedge friction (triage, determinism, onboarding).

---

## 0) Context & Observations

Assay's differentiator is a closed-loop governance workflow:

**Observe** -> **Generate/Profile** -> **Lock** -> **Gate PRs** -> **Export verifiable evidence**

Plus runtime enforcement (MCP proxy / sandbox) as defense-in-depth.

The core architecture is strong (replay/fingerprinting, evidence integrity, security hardening). The biggest risks are in CLI layer and boundary glue: duplication, fragile error classification, config-format inconsistencies, and command coupling.

**Primary product wedge (must win):**
1. Deterministic CI replay + gating (< 5 min to first PR gate)
2. Evidence bundles + packs as compliance primitives
3. Supply-chain discipline for policy content (lockfiles, verification)

---

## 1) Design Invariants (Must Not Break)

Every change in this RFC must preserve these constraints.

**I1 - Determinism as default**
CI outcomes must be deterministic for identical inputs. Replay must stay explicit and reproducible (trace/policy/toolchain/seed discipline).

*Acceptance invariant*: same bundle + same flags => identical subset outcomes.

**I2 - Bundle/Evidence integrity is sacrosanct**
Canonicalization + hashing/signature semantics must not change accidentally. `json_strict`/canonicalization stays part of the security model.

*Acceptance invariant*: existing evidence verification stays compatible (or is versioned).

**I3 - Hermetic offline default**
Offline replay/CI must not introduce network dependencies without explicit opt-in.

**I4 - Fail-closed on security-sensitive surfaces**
Scrubbing/verify/pack loading: no silent bypass for invalid encodings, extra entries, etc.

**I5 - Compatibility surfaces stay stable**
`run.json`/`summary.json`, SARIF/JUnit, GitHub Action contract: version- and migration-aware. CLI changes must not create silent contract breaks.

---

## 2) Key Claims (What Is SOTA - Lean Into These)

These are engineering strengths that distinguish Assay. Refactors are only good if they reduce wedge friction without damaging these.

1. **Wilson-lower-bound gating** for auto-allow decisions (with separate display score) - `generate.rs`, `profile.rs`
2. **Content-addressed replay** with typed request keys + schema versioning + cache busting - `vcr/`, `engine/runner.rs`
3. **Typed VCR + JCS canonicalization** instead of raw HTTP matching
4. **Evidence integrity chain** separating metadata from payload integrity - `assay-evidence` manifest, SHA-256, Merkle root
5. **Adaptive judge (SPRT-inspired)** + seed-based blind labeling
6. **Security hardening**: terminal sanitization state machine, sim/chaos attacks, strict JSON handling

---

## 3) Critical Debt Inventory (Ranked by ROI)

### D1 - Fragile error classification (string matching)

**Risk**: correctness regressions, non-deterministic triage, upstream message drift breaks reason mapping.
**Why it matters**: drives exit codes, CI gating, supportability.

Current state (`run_output.rs`): multiple `.contains()` branches mapping message substrings to `ReasonCode`.

**Fix direction**: typed error boundary + ReasonCode mapping on enum variants (not substring matching) at the core->cli boundary.

```rust
// crates/assay-core/src/errors.rs (boundary type)
pub enum RunError {
    TraceNotFound(PathBuf),
    ConfigParse { path: PathBuf, detail: String },
    ProviderRateLimit { status: u16 },
    ProviderTimeout,
    ProviderServer { status: u16 },
    Network(String),
    JudgeUnavailable,
}
```

CLI maps `RunError` variants to `ReasonCode` via `match`. Core internals may still use `anyhow`; boundary should be typed.

### D2 - Run vs CI flow duplication

**Risk**: behavior drift, double maintenance, regressions when extending features.
**Why it matters**: CI wedge (SARIF/JUnit/reporting) evolves quickly; duplication slows shipping.

Current state: `run.rs` and `ci.rs` share most flow but diverge in local copies.

**Fix direction**: one shared pipeline with CI as renderer layer.

```rust
// commands/pipeline.rs
pub async fn run_pipeline(opts: PipelineOpts) -> Result<(RunOutcome, RunArtifacts)> {
    let runner = build_runner(&opts)?;
    let artifacts = runner.run_suite().await?;
    let outcome = decide_run_outcome(&artifacts, &opts);
    write_core_outputs(&outcome, &artifacts, &opts)?;
    Ok((outcome, artifacts))
}
```

### D3 - commands/mod.rs coupling + replay dependency

**Risk**: refactor lock-in, high complexity, low testability.
**Why it matters**: every DX feature touches dispatch/pipeline.

Prior work reduced `commands/mod.rs` substantially, but replay still depends on `super::` business re-exports.

**Fix direction**: introduce `cli::pipeline`; make replay depend on pipeline API, not `commands/mod.rs` internals. Keep `commands/mod.rs` as routing + wiring only.

### D4 - Unsafe env mutation (`set_var`)

**Risk**: UB/races in multithread context; observability/debugging pain.
**Why it matters**: CI reliability and future async expansion.

Current state: multiple `std::env::set_var` call sites.

**Fix direction**: parse env once at startup and thread explicit options through call chain.

```rust
pub struct RunOptions {
    pub vcr_mode: Option<VcrMode>,
    pub otel_endpoint: Option<String>,
    pub log_level: Option<String>,
}
```

### D5 - Inconsistent config versioning/templates

**Risk**: onboarding confusion, drift between init/docs/parser.

Current state: mixed `version` shapes/values across templates.

**Fix direction**: read-compatible, write-canonical.
- Eval config: `configVersion: 1` (canonical key, int)
- Policy: `version: "1.0"` (string)

### D6 - "Pack" naming collision

**Risk**: user confusion and docs complexity.

Current state:
- `assay init --pack ...` means scaffold presets
- `assay evidence lint --pack ...` means compliance packs

**Fix direction**: rename init `--pack` to `--preset` (or `--template`); reserve "pack" for compliance packs.

---

## 4) Proposed Execution Plan (3 Waves, With Stop Lines)

### Wave A - Correctness & Contract Safety

**Goal**: deterministic triage + safe flags + canonical config output.
**Size**: small, high impact.

| Task | Files | Estimate |
|------|-------|----------|
| A1: Typed error boundary + ReasonCode mapping | new/modify core errors + run/ci/run_output | ~200 new, ~100 removed |
| A2: Replace `set_var` with explicit run options | run/ci/builder/main call chain | ~150 changed |
| A3: Canonical config writing in init/templates | templates/init/docs | ~30 changed |

**Acceptance criteria**:
- [ ] No substring-based reason mapping in run/ci hot paths (or only legacy fallback)
- [ ] Mapping tests: config parse, missing trace, missing baseline, auth/network failures
- [ ] No env mutation in CLI strict-mode path
- [ ] `init` writes canonical versions; docs aligned
- [ ] `cargo test --workspace` green
- [ ] No new dependencies

**Stop line**: no broad error-stack rewrite; no pipeline unification in this wave.

### Wave B - Maintainability

**Goal**: reduce duplication and unblock modularization.
**Prerequisite**: Wave A merged.

| Task | Files | Estimate |
|------|-------|----------|
| B1: Shared `run_pipeline` for run and ci | new `commands/pipeline.rs`, modify run/ci/replay | ~250 new, ~300 removed |
| B2: Reduce `commands/mod.rs` to dispatch only | mod.rs + replay | ~20 removed |
| B3: Rename `--pack` -> `--preset` on init | args/init/templates/docs | ~40 changed |

**Acceptance criteria**:
- [ ] run and ci share core execution flow (renderer-only differences)
- [ ] Golden tests confirm equivalent outcomes where expected
- [ ] `commands/mod.rs` small and free of business logic
- [ ] Replay path no longer imports business helpers from `commands/mod.rs`
- [ ] `--pack` reserved for compliance/evidence commands
- [ ] `cargo test --workspace` green

**Stop line**: no perf rewrites; no output-contract behavior change.

### Wave C - Performance / Scale (Data-triggered)

**Goal**: optimize only with evidence.

| Task | Trigger | Files |
|------|---------|-------|
| C1: Single-pass streaming verify+lint | verify/lint CI cost > 5s | evidence lint/verify engine |
| C2: `RunnerRef` shared refs | profiling shows clone overhead | core runner |
| C3: Profile store batch ops | >10k profile entries | storage store |
| C4: Stable run-id tracking beyond ring buffer | corpus growth pressure | storage layer |

**Acceptance criteria**:
- [ ] Benchmarked improvements on realistic workloads
- [ ] No regression in determinism/integrity
- [ ] Criterion coverage updated

**Stop line**: do not start without measured bottleneck evidence.

---

## 5) Best-Practice Alignment (Feb 2026)

- Typed errors at boundaries stabilize automation surfaces and avoid message-drift regressions.
- Single execution pipeline with renderer overlays is standard for CLI reliability.
- Read-compat/write-canonical is best practice for schema evolution.
- Avoid mutable process-wide globals (env vars) in async/multithread runtime paths.

Security posture retained:
- Fail-closed verification behavior
- Hermetic offline defaults
- Sanitization + strict/canonical JSON as first-class controls

---

## 6) Recommended Next Steps

1. Execute **Wave B1** (`run_pipeline`) to remove run/ci duplication on the core execution path.
2. Follow with **Wave B2/B3** (coupling reduction + init `--pack` rename migration).
3. Keep Wave C explicitly metrics-gated.

---

## Appendix A - Scope Guardrails (What We Will Not Do)

- No kernel enforcement/eBPF expansion as primary DX roadmap item.
- No one-shot "rewrite all errors" migration.
- No schema breaks to run/summary/SARIF/JUnit without versioning + migration notes.
- No new watcher backend dependencies before Wave B stability.
- No evidence bundle format v2 in this track.
- No full codebase `thiserror` migration; only core->cli boundary typing.
- No cross-platform atomic write rewrite in this RFC scope.
- No broad Arc-free runner rewrite without scale evidence.

---

## Decision Record

- 2026-02-07: Draft created from codebase audit + owner review. Wave A scoped for immediate execution.
- 2026-02-08: Wave A merged to `main` (`#198`, `#202`). Wave B started.
