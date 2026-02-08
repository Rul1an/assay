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

### Wave C — Performance / Scale (Data-triggered)

**Goal**: Optimize only with measured evidence. No speculative performance work.
**Prerequisite**: Wave B merged. C0 (harness) must land before any C1–C4 work starts.

#### C0: Reproducible perf harness + budgets (required first)

Without a harness, the "data-triggered" claim is unenforceable. This is the smallest investment with the biggest payoff — it makes all other C-tasks reviewable.

**Deliverables**:
- Fixture generator for bundles/events/profile corpora at defined workload classes:
  - `small`: 1MB bundle, 1k events, 10 rules
  - `typical-pr`: 10MB bundle, 10k events, 50 rules
  - `large`: 50MB+ bundle, 100k+ events, 500+ rules
- Criterion benches: `cargo bench -p assay-evidence -- verify_lint`
- Performance budgets document: p50/p95 targets per workload class, per runner (ubuntu-latest baseline)

**Files**: new bench in `assay-evidence/benches/`, fixture generator, `docs/PERFORMANCE-BUDGETS.md`

#### C1: Single-pass streaming verify+lint

**Trigger** (any of):
- verify+lint p95 > 5s on ubuntu-latest for `large` workload class (>=50MB or >=100k events or >=500 rules)
- verify+lint p50 > 2s on `typical-pr` workload class

**Scope definition** — "single-pass" means one decompress + one tar walk:
1. Read `manifest.json`
2. Stream entries (no full buffer in memory)
3. Verify hashes/sizes per entry against manifest
4. Scan for forbidden patterns
5. Collect lint events
6. Produce identical error/warning set as current multi-pass

**Invariant guardrails** (protects I2 + I4):
- `VerifyLimits` (max entry size, max total uncompressed, max files) remain enforced — streaming is the opportunity to make limits _better_, not weaker
- Golden tests: verify+lint output on reference bundles must be byte-identical before and after
- No semantic changes to verify/lint outputs; only performance and memory behavior may change

**Files**: `assay-evidence/src/lint/engine.rs`, `assay-evidence/src/verify.rs`

#### C2: `RunnerRef` shared refs (no per-task clone)

**Trigger**:
- Runner clone/build overhead > 10% of total suite runtime on a suite of >=1000 tests

**Measurement points** (add behind `debug`/`perf` feature flag):
- `runner_build_ms`: time to construct runner
- `runner_clone_count`: number of Arc field clones per suite
- `runner_clone_ms`: cumulative clone time

Current state: 6 `.clone()` calls on Arc-wrapped fields per task (`engine/runner.rs:529-543`, `768-791`). At current suite sizes (<100 tests) this is negligible.

**Files**: `assay-core/src/engine/runner.rs`

#### C3: Profile store scaling

**Trigger** (any of):
- Profile merge of 1 run > 1s p95 at >=10k entries
- Profile load > 500ms p95

**Decision required before implementation**: identify the actual bottleneck:
- Write path: merge/update complexity
- Read path: lookups per event (hot path)
- Serialization: load/serialize cost

**Storage strategy options** (decide based on profiling, not upfront):
- SQLite (already used for eval DB — one DB or two?)
- Append-only log + compaction
- Current YAML with batch operations

**Files**: `assay-core/src/storage/store.rs` (884 lines, no in-file tests — tests should be added as part of this work regardless of which storage strategy is chosen)

#### C4: Stable run-id tracking beyond ring buffer

**Trigger**: Profile corpus growth causes ring buffer evictions that break replay determinism or cause duplicate-merge errors.

**Invariant guardrail** (protects I1):
- Double-merge of same `run_id` must be impossible across N runs (define N as a hard bound)
- Replacing the ring buffer must not break determinism or introduce memory blowups

**Bounded structure options** (choose one):
- Stable hash-set on disk (SQLite) — deterministic, no false positives, proven
- Bloom filter + periodic reset with epoch — space-efficient but false positives affect UX (false "already merged" errors); only acceptable if error path is graceful

**Files**: `assay-core/src/storage/`

#### Acceptance criteria (all C tasks)

- [ ] C0 harness exists and produces reproducible results before any C1–C4 work starts
- [ ] Benchmarked improvement on the relevant workload class (p50 and p95)
- [ ] Golden tests prove semantic equivalence of outputs (verify/lint/run results)
- [ ] No regression in determinism (I1) or integrity (I2)
- [ ] Criterion benches updated with before/after
- [ ] No new dependencies without justification

#### Non-goal

No semantic changes to verify/lint/run outputs. Only performance and memory behavior may change, and must be proven equivalent via golden tests on reference fixtures.

**Stop line**: Do not start C1–C4 without C0 harness in place. Do not start any C-task without measured bottleneck evidence on the defined workload classes.

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
- No semantic changes to verify/lint/run outputs in Wave C; only performance/memory behavior, proven equivalent via golden tests.

---

## Decision Record

- 2026-02-07: Draft created from codebase audit + owner review. Wave A scoped for immediate execution.
- 2026-02-08: Wave A merged to `main` (`#198`, `#202`). Wave B started.
- 2026-02-08: Wave B1 opened as `#204` (shared run/ci pipeline); Wave B2 branch extracted dispatch logic from `commands/mod.rs` into `commands/dispatch.rs`.
- 2026-02-08: Wave C rewritten with concrete triggers (workload classes, percentiles, runner platform), C0 harness prerequisite, scope guardrails for C1 (streaming invariants), and measurable thresholds for C2-C4.
- 2026-02-08: Wave C runner overhead instrumentation added as additive summary metrics (`runner_clone_ms`, `runner_clone_count`) to make C2 trigger measurable on real suites.
