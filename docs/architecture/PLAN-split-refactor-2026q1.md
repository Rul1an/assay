# Plan: Refactor Hotspots (Q1-Q2 2026)

> Status: Proposed
> Date: 2026-02-13
> Scope: Largest handwritten Rust production files and related CI/CD gates
> Constraint: No behavior drift in CLI/public contracts; incremental mergeable PRs

## 1) Baseline (HEAD: 6ae1d340)

Largest production hotspots (tests/generated excluded from priority, sorted by LOC):

| File | LOC | Functions | Test attrs | unwrap/expect | unsafe |
|---|---:|---:|---:|---:|---:|
| `crates/assay-evidence/src/bundle/writer.rs` | 1442 | 37 | 11 | 41 | 0 |
| `crates/assay-registry/src/verify.rs` | 1065 | 44 | 26 | 55 | 0 |
| `crates/assay-core/src/explain.rs` | 1057 | 21 | 4 | 2 | 0 |
| `crates/assay-core/src/runtime/mandate_store.rs` | 1046 | 38 | 21 | 84 | 0 |
| `crates/assay-core/src/engine/runner.rs` | 1042 | 31 | 3 | 8 | 0 |
| `crates/assay-core/src/providers/trace.rs` | 881 | 30 | 18 | 1 | 0 |
| `crates/assay-registry/src/lockfile.rs` | 863 | 31 | 16 | 12 | 0 |
| `crates/assay-registry/src/cache.rs` | 844 | 35 | 16 | 39 | 0 |
| `crates/assay-cli/src/cli/commands/monitor.rs` | 833 | 15 | 3 | 2 | 7 |

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
- Enforce feature matrix per touched crate:
  - `cargo test -p <crate> --no-default-features`
  - `cargo test -p <crate> --features <curated_combo_1>`
  - `cargo test -p <crate> --features <curated_combo_2>`
  - `cargo test -p <crate> --all-features`
- For hotspot crates only: run feature drift sweep with `cargo-hack` and execute via `cargo-nextest` where practical for runtime budget.
  - Example: `cargo hack test -p <crate> --each-feature`
  - Example: `cargo nextest run -p <crate> --all-features`
- Add semver gate only for published/downstream-facing library crates:
  - `cargo semver-checks check-release -p <crate> --baseline-rev <pinned_main_sha>`
  - Pin `<pinned_main_sha>` at sprint start to avoid moving-baseline noise.
- Add clippy anti-placeholder gate:
  - `-D clippy::todo -D clippy::unimplemented`
- Add nightly security/stability lane (non-blocking initially):
  - `cargo miri test` for selected target tests only (focused, low-flake subset).
  - fuzz smoke jobs for parser/crypto entry points.
  - Kani lane as opt-in until proof burden and harness cost are justified.

### Behavior freeze contracts (explicit per hotspot)

- `verify.rs`: `VerifyError::Code` mapping, fail-closed decisions, digest normalization invariants.
- `writer.rs`: deterministic bundle encoding invariants, manifest/events ordering, strict size-limit errors.
- `runner.rs`: outcome status mapping, retry accounting, baseline comparison outputs.
- `mandate_store.rs`: state transition invariants (`upsert -> consume -> revoke`), monotonic use-count behavior.
- `monitor.rs`: syscall fallback behavior and event decision invariants.
- `trace.rs`: parse/normalize error invariants and event shape guarantees.

### Exit criteria

- Green CI with new gates on unchanged code.
- Baseline performance snapshots stored for touched benches.

## Wave 1: Security-first split (`verify.rs`, `writer.rs`)

Step status:

- Writer split: merged via PR #332.
- Verify split: pending completion.
- Verify Step1 behavior freeze (tests/docs/gates): in progress on `codex/wave5-step1-verify-freeze`.

### A. `crates/assay-registry/src/verify.rs`

Target structure:

```text
verify/
  mod.rs        # public API only
  wire.rs       # wire-format parsing: base64/JSON shape/header strictness
  digest.rs     # digest parsing + strict compare
  dsse.rs       # envelope parse/PAE/verify
  keys.rs       # key selection + key-id matching
  policy.rs     # accept/reject policy (no crypto deps)
  errors.rs
  tests/
```

Trust boundary rules:

- `policy.rs` is crypto-agnostic (decision logic only).
- `dsse.rs` is policy-agnostic (verification/parsing only).
- `mod.rs` exports API and orchestrates, but contains no crypto/wire internals.

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
bundle/writer_next/
  mod.rs
  write.rs
  verify.rs
  manifest.rs
  events.rs
  tar_write.rs
  tar_read.rs
  limits.rs
  errors.rs
  tests.rs
```

Contract boundaries:

- `write.rs`: BundleWriter write orchestration only (no verify-path decisions).
- `verify.rs`: verify orchestration only (no write-path orchestration).
- `tar_write.rs`: deterministic archive encoding only.
- `tar_read.rs`: tar/gzip read + safe iteration helpers only.
- `limits.rs`: single source of truth for max sizes and bounded readers.
- `events.rs`: NDJSON normalization/canonicalization rules only.
- `errors.rs`: typed errors/codes and mapping helpers only (no parsing/IO ownership).

Execution discipline:

- Step 3 Commit A/B/C are mechanical split + docs/gates only.
- No perf tuning or behavior changes in mechanical commits.
- Perf work lands only in a follow-up step after mechanical split is merged.

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

Step status:

- Step 1 (behavior freeze + inventory + drift gates): implemented on `codex/wave2-step1-behavior-freeze` (inventory, contract tests, checklists, reviewer script).
- Step 2 (mechanical split): merged via PR #336 (Commit A scaffolds + Commit B mechanical function moves behind stable facades).

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
- Add concurrency model checks for state transitions with pragmatic lanes:
  - Loom only on small, purpose-built harnesses for race-sensitive state transitions.
  - Miri on selected tests that exercise ownership/aliasing-sensitive paths.
  - Kani as opt-in lane for critical invariants where harnessing cost is justified.

### Exit criteria (Wave 2)

- Existing bench budgets (`store_write_heavy`, `suite_run_worstcase`) non-regressive or improved.
- New concurrency invariants covered by deterministic tests and model tests.

## Wave 3: Unsafe and parser boundary hardening (`monitor.rs`, `trace.rs`)

Step status:

- Step 1 (behavior freeze + inventory + drift gates): merged via PR #337.
- Step 2 (mechanical split): merged via PR #338.

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
- Add unsafe policy gate:
  - `#![deny(unsafe_op_in_unsafe_fn)]`
  - `rg "unsafe" crates/assay-cli/src/cli/commands/monitor.rs` must only match `syscall_linux.rs`.

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

Step status:

- Step 1 (`lockfile.rs` + `cache.rs` behavior freeze/inventory/gates): merged via PR #339.
- Step 2 (`lockfile.rs` + `cache.rs` mechanical split): merged via PR #340.
- Step 2.x (`cache.rs` facade thinness follow-up for read/evict/list/get_metadata): merged via PR #343.
- Step 3 (`explain.rs` mechanical split behind stable facade): merged via PR #344.
- Promotion to `main` (Wave4 closure): merged via PR #345.

### Wave 4 outcome (merged on `main`)

- PR chain: `#339` -> `#340` -> `#343` -> `#344` -> `#345`.
- Facade hotspots reduced from 2764 LOC to 1252 LOC (~54.7% reduction).
- `explain.rs` reduced from 1057 LOC to 11 LOC (thin facade).
- Boundaries are enforced by reviewer scripts:
  - `scripts/ci/review-wave4-step2.sh`
  - `scripts/ci/review-wave4-step3.sh`

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

1. Artifact attestation as a required pair:
   - produce provenance in release workflow (`attest-build-provenance`)
   - verify attestation in CI/release validation; fail closed if missing/invalid
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
- CI policy note: toolchain stays pinned to stable in workflows; release-note links above are compatibility context, not floating requirements.
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
