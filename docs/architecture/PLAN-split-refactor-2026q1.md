# Plan: Refactor Hotspots (Q1-Q2 2026)

> Status: Proposed
> Date: 2026-02-13
> Scope: Largest handwritten Rust production files and related CI/CD gates
> Constraint: No behavior drift in CLI/public contracts; incremental mergeable PRs

## 1) Baseline (HEAD: 6ae1d340)

Largest production hotspots (tests/generated excluded from priority):

| File | LOC | Functions | Test attrs | unwrap/expect | unsafe |
|---|---:|---:|---:|---:|---:|
| `crates/assay-evidence/src/bundle/writer.rs` | 1442 | 37 | 11 | 41 | 0 |
| `crates/assay-registry/src/verify.rs` | 1065 | 44 | 26 | 55 | 0 |
| `crates/assay-core/src/runtime/mandate_store.rs` | 1046 | 38 | 21 | 84 | 0 |
| `crates/assay-core/src/engine/runner.rs` | 1042 | 31 | 3 | 8 | 0 |
| `crates/assay-core/src/providers/trace.rs` | 881 | 30 | 18 | 1 | 0 |
| `crates/assay-registry/src/lockfile.rs` | 863 | 31 | 16 | 12 | 0 |
| `crates/assay-registry/src/cache.rs` | 844 | 35 | 16 | 39 | 0 |
| `crates/assay-cli/src/cli/commands/monitor.rs` | 833 | 15 | 3 | 2 | 7 |
| `crates/assay-core/src/explain.rs` | 1057 | 21 | 4 | 2 | 0 |

## 2) Prioritization

Priority score is based on:

1. Security and correctness risk (crypto/parsing/unsafe/concurrency paths)
2. Runtime criticality (hot path or high IO churn)
3. Refactor payoff (cohesion increase + testability increase)
4. Blast radius (ability to land in small, reversible steps)

Execution order:

1. `verify.rs` + `writer.rs` (security + parser/crypto boundaries)
2. `runner.rs` + `mandate_store.rs` (state/concurrency/perf)
3. `monitor.rs` + `trace.rs` (unsafe/syscall isolation + parser isolation)
4. `lockfile.rs` + `cache.rs` + `explain.rs` (consolidation and cleanup)

## 3) Refactor Waves

## Wave 0: Guardrails first (required before major splits)

### Objectives

- Freeze behavior before decomposition.
- Catch regressions in API, features, security posture, and performance budget.

### Work

- Add split-contract checks per module (grep-gates for forbidden imports/couplings).
- Enforce matrix per touched crate:
  - `cargo test -p <crate> --no-default-features`
  - `cargo test -p <crate> --all-features`
- Add semver gate for published library crates:
  - `cargo semver-checks check-release -p <crate> --baseline-rev origin/main`
- Add clippy anti-placeholder gate:
  - `-D clippy::todo -D clippy::unimplemented`
- Add nightly security/stability lane (non-blocking initially):
  - `cargo miri test` for selected crates/targets with supported tests.
  - fuzz smoke jobs for parser/crypto entry points.

### Exit criteria

- Green CI with new gates on unchanged code.
- Baseline performance snapshots stored for touched benches.

## Wave 1: Security-first split (`verify.rs`, `writer.rs`)

### A. `crates/assay-registry/src/verify.rs`

Target structure:

```text
verify/
  mod.rs        # public API only
  digest.rs     # digest parsing + strict compare
  dsse.rs       # envelope parse/PAE/verify
  keys.rs       # key selection + key-id matching
  policy.rs     # accept/reject policy (no crypto deps)
  errors.rs
  tests/
```

Performance improvements:

- Avoid repeated canonicalization and base64 decode passes.
- Reuse parsed/canonical buffers where safe.

Security improvements:

- Fail-closed reason mapping remains deterministic.
- Centralize signature checks and key-id checks in one boundary.
- Add property tests for determinism and malformed envelope handling.

### B. `crates/assay-evidence/src/bundle/writer.rs`

Target structure:

```text
bundle/writer/
  mod.rs
  manifest.rs
  events.rs
  tar_io.rs
  limits.rs
  verify.rs
  errors.rs
  tests/
```

Performance improvements:

- Reduce transient allocations in bundle finalize/verify path.
- Stream NDJSON read/validate with strict limits and bounded buffers.

Security improvements:

- Keep hard size limits as first-class checks.
- Strengthen malformed tar/manifest/event corpus tests and fuzz seeds.

### Exit criteria (Wave 1)

- No contract drift in existing integration tests.
- Parser/crypto fuzz smoke + property tests green.
- No measurable regression >5% median for existing verify/lint benchmarks.

## Wave 2: Runtime decomposition (`runner.rs`, `mandate_store.rs`)

### A. `crates/assay-core/src/engine/runner.rs`

Target structure:

```text
engine/runner/
  mod.rs
  execute.rs      # orchestration flow
  retry.rs        # retry classification/backoff
  baseline.rs     # baseline compare paths
  scoring.rs      # judge/semantic enrichment
  cache.rs        # runner-local cache interaction
  errors.rs
```

Performance improvements:

- Remove duplicate transformations on results/attempt metadata.
- Reduce repeated IO path branching in happy path.

Security/correctness improvements:

- Explicit error taxonomy to avoid accidental status remapping.
- Deterministic outcome mapping tests.

### B. `crates/assay-core/src/runtime/mandate_store.rs`

Target structure:

```text
runtime/mandate_store/
  mod.rs
  schema.rs
  upsert.rs
  consume.rs
  revocation.rs
  stats.rs
  txn.rs
  tests/
```

Performance improvements:

- Consolidate statement preparation strategy.
- Shorten lock hold-time around DB operations.

Security/correctness improvements:

- Explicit transaction invariants for consume/revoke flows.
- Add concurrency model checks for state transitions (loom-focused harness where feasible).

### Exit criteria (Wave 2)

- Existing bench budgets (`store_write_heavy`, `suite_run_worstcase`) non-regressive or improved.
- New concurrency invariants covered by deterministic tests and model tests.

## Wave 3: Unsafe and parser boundary hardening (`monitor.rs`, `trace.rs`)

### A. `crates/assay-cli/src/cli/commands/monitor.rs`

Target structure:

```text
monitor/
  mod.rs
  policy_compile.rs
  inode_resolve.rs
  runtime.rs
  events.rs
  syscall_linux.rs   # all unsafe isolated here
  tests/
```

Performance improvements:

- Move repeated inode/path resolution out of event hot loop where possible.

Security improvements:

- Reduce unsafe surface to one module with safe wrappers.
- Add negative tests for syscall fallback behavior.

### B. `crates/assay-core/src/providers/trace.rs`

Target structure:

```text
providers/trace/
  mod.rs
  parse.rs
  normalize.rs
  provider.rs
  io.rs
  tests/
```

Performance improvements:

- Single-pass parse/normalize where possible.
- Fewer intermediate allocations in JSON/event conversion.

Security improvements:

- Strict malformed input handling preserved through golden tests.

### Exit criteria (Wave 3)

- Unsafe LOC in command module reduced materially and isolated.
- Parser corpus coverage increased; no panic paths on malformed inputs.

## Wave 4: Consolidation (`lockfile.rs`, `cache.rs`, `explain.rs`)

### Objectives

- Improve maintainability without churn in public UX.
- Capture low-risk/high-payoff split debt.

### Work

- `lockfile.rs` split into parse/io/generate/verify/update modules.
- `cache.rs` split into keys/policy/io/integrity/eviction modules.
- `explain.rs` split into render/model/source/diff modules.

### Exit criteria

- Each file under soft 800 LOC target unless justified by cohesive domain.
- Contract tests unchanged; grep-gates enforce boundaries.

## 4) CI/CD improvements linked to this plan

Already strong today:

- Pinned actions by full SHA.
- `permissions: {}` default with job-scoped elevation.
- `cargo-audit` + `cargo-deny`.
- Criterion + Bencher lanes.

Additions for this refactor program:

1. Artifact attestation in release workflow (`attest-build-provenance`) and verification step in release validation.
2. Dedicated split-gate workflow for feature matrix + semver + anti-placeholder lints.
3. Nightly fuzz/model lane for parser/crypto/concurrency hotspots (non-blocking first, then required for touched paths).
4. Fast test execution path with `cargo-nextest` for broader matrix coverage time budget.

## 5) Definition of Done per split PR

Each split PR must include:

1. Fresh hotspot inventory output from HEAD.
2. Behavior-freeze tests (or proof existing coverage is equivalent).
3. Boundary grep-gates and rationale for forbidden couplings.
4. Performance before/after snippet (median, p95 where available).
5. Security note: threat and invariant impact (what became easier to verify).

## 6) Non-goals

- No API redesign unrelated to hotspot decomposition.
- No policy contract changes.
- No broad dependency churn unless needed for a specific gate.

## 7) External SOTA references (reviewed Feb 2026)

- [Rust 2024 edition guide](https://doc.rust-lang.org/edition-guide/rust-2024/index.html), with [Rust 1.85.0 release context](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Rust Performance Book](https://nnethercote.github.io/perf-book/)
- [RustSec advisory database](https://github.com/RustSec/advisory-db), [cargo-audit](https://docs.rs/crate/cargo-audit/latest), [cargo-deny](https://embarkstudios.github.io/cargo-deny/)
- [Loom](https://github.com/tokio-rs/loom) (concurrency permutation testing), [Kani](https://github.com/model-checking/kani) (model checking), [Miri](https://github.com/rust-lang/miri) (UB detection)
- [cargo-nextest](https://nexte.st/) for faster/more reliable large test suites
- [SLSA v1.2 requirements](https://slsa.dev/spec/v1.2/requirements), [GitHub artifact attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/use-artifact-attestations), [GitHub OIDC hardening](https://docs.github.com/en/actions/security-for-github-actions/security-hardening-your-deployments/about-security-hardening-with-openid-connect)
- Recent Rust verification/fuzzing/security literature:
  - [FRIES (ISSTA 2024)](https://dl.acm.org/doi/10.1145/3650212.3680354)
  - [Thrust (PLDI 2025)](https://dl.acm.org/doi/10.1145/3729250)
  - [Converos (USENIX ATC 2025)](https://www.usenix.org/conference/atc25/presentation/zhou-mingdeng)
  - [“Does Safe == Secure?” (USENIX Security 2025 poster)](https://www.usenix.org/conference/usenixsecurity25/poster-session/does-safe-secure)
