# SPLIT PLAN - Wave53 Top 2-9 Rust Hotspot Refactor

## Intent

Refactor the selected top 2 through 9 handwritten Rust hotspots behind stable facades in one
coordinated wave.

This wave follows the user-selected top 2 through 9 set from the prior hotspot snapshot, excluding
generated `crates/assay-ebpf/src/vmlinux.rs`. Later repo-root inventories may move other files above
or below this set; Wave53 scope must not drift just because a later branch changes LOC ordering. The
goal is reviewability and ownership clarity, not feature work.

## Scope

| Rank | File | Baseline LOC | Crate | Readiness | Target shape |
| ---: | --- | ---: | --- | --- | --- |
| 2 | `crates/assay-runner-core/src/kernel.rs` | 992 | `assay-runner-core` | Medium | stable `kernel.rs` facade over `kernel/*` |
| 3 | `crates/assay-cli/src/cli/commands/runner_spike.rs` | 686 | `assay-cli` | Medium | command facade over `runner_spike/*` |
| 4 | `crates/assay-ebpf/src/main.rs` | 559 | `assay-ebpf` | Low | conservative eBPF helpers split, tracepoint names unchanged |
| 5 | `crates/assay-registry/src/lockfile.rs` | 649 | `assay-registry` | High | finish thin facade over existing `lockfile_next/*` |
| 6 | `crates/assay-core/src/mcp/policy/mod.rs` | 636 | `assay-core` | Low | policy public surface facade, no policy behavior drift |
| 7 | `crates/assay-cli/src/cli/commands/bundle.rs` | 632 | `assay-cli` | High | command facade over `bundle/*` |
| 8 | `crates/assay-core/src/report/summary.rs` | 629 | `assay-core` | High | report facade over `summary/*` |
| 9 | `crates/assay-cli/src/cli/commands/doctor.rs` | 629 | `assay-cli` | Medium | command facade over `doctor/*` |

`crates/assay-ebpf/src/vmlinux.rs` remains out of scope because it is generated kernel binding
surface.

Current repo-root inventory on `codex/runner-otel-slice12-harness` shows additional large files
above `crates/assay-ebpf/src/main.rs`, including evidence schema-generation files. Those are not
added to Wave53 without a separate scope decision.

## Relation To Existing Waves

Wave51 covers different large hotspots:

- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-cli/src/cli/commands/sandbox.rs`
- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-evidence/src/trust_basis.rs`

Wave53 must not take over Wave51 scope. Wave53 may reuse its gate style: freeze first, keep facades,
move bodies mechanically, and close with review artifacts.

Older waves already touched two Wave53 files:

- `lockfile.rs` has an existing `lockfile_next/*` direction from Wave4. Wave53 should finish facade
  thinning and closure only.
- `mcp/policy/mod.rs` has existing policy-engine and MCP policy history. Wave53 must treat it as a
  contract file, not a cleanup target.

## Standing Rules

1. Preserve public APIs, command names, exit codes, JSON output, CloudEvent/MCP shapes, reason codes,
   policy semantics, and eBPF tracepoint names.
2. Keep the current top-level files as stable facades.
3. Move implementation bodies 1:1 before any cleanup.
4. Keep module visibility crate-private unless an existing public API already requires broader
   visibility.
5. Do not edit `.github/workflows/`.
6. Do not change generated `vmlinux.rs`.
7. Do not mix performance tuning, dependency changes, formatting churn, or behavior cleanup into
   mechanical split commits.
8. Every step gets a checklist, move-map, review-pack, and `scripts/ci/review-wave53-...sh` gate.

## Frozen Public Surface

Wave53 freezes the following surfaces.

For `kernel.rs`:

- `KernelLayerEvent`
- `KernelLayerCapture`
- `KernelLayerError`
- `KernelLayerBuilder`
- event decoding and health-note meaning

For `runner_spike.rs`:

- `RunnerSpikeArgs`
- `RunnerSpikeCommand`
- `RunnerSpikeRunArgs`
- CLI output, bundle output paths, cgroup behavior, kernel-capture env wiring, and exit-status
  mapping

For `assay-ebpf/src/main.rs`:

- tracepoint program names
- map names and event struct compatibility
- open/connect/fork event meaning
- loader-path filtering and dedup semantics

For `lockfile.rs`:

- `Lockfile`
- `LockedPack`
- `LockSource`
- `LockSignature`
- `VerifyLockResult`
- `LockMismatch`
- parse, format, digest, load, save, check, update behavior

For `mcp/policy/mod.rs`:

- `McpPolicy`
- `EnforcementSettings`
- `ToolPolicy`
- `PolicyEvaluation`
- `PolicyDecision`
- `TypedPolicyDecision`
- `PolicyObligation`
- `ApprovalArtifact`
- contract structs, reason-code strings, deserialization compatibility, and pattern matching

For `bundle.rs`:

- `bundle create` and `bundle verify` behavior
- run-id and seed extraction
- input-coverage enforcement
- source-root/path selection
- archive contents and missing-input handling

For `summary.rs`:

- `Summary`
- `SarifOutputInfo`
- `Seeds`
- `JudgeMetrics`
- `Provenance`
- `ResultsSummary`
- `PerformanceMetrics`
- `SlowestTest`
- `PhaseTimings`
- `judge_metrics_from_results`
- `write_summary`

For `doctor.rs`:

- diagnostics rendering
- fix preview/apply behavior
- suggested-patch matching
- trace fix targets
- YAML unknown-field repair behavior

## Planned Layout

Step2 high-readiness split:

```text
crates/assay-registry/src/lockfile.rs
crates/assay-registry/src/lockfile_next/
  types.rs

crates/assay-core/src/report/summary.rs
crates/assay-core/src/report/summary/
  types.rs
  metrics.rs
  writer.rs

crates/assay-cli/src/cli/commands/bundle.rs
crates/assay-cli/src/cli/commands/bundle/
  implementation.rs
  verify.rs
  paths.rs
  coverage.rs
```

Step3 CLI medium-readiness split:

```text
crates/assay-cli/src/cli/commands/runner_spike.rs
crates/assay-cli/src/cli/commands/runner_spike/
  args.rs
  implementation.rs
  spec.rs
  phases.rs
  cgroup.rs
  logs.rs
  exit_status.rs

crates/assay-cli/src/cli/commands/doctor.rs
crates/assay-cli/src/cli/commands/doctor/
  implementation.rs
  fixes.rs
  patching.rs
  parse_error.rs
```

Step4 runner/eBPF split:

```text
crates/assay-runner-core/src/kernel.rs
crates/assay-runner-core/src/kernel/
  decode.rs
  stats.rs
  health.rs
  notes.rs

crates/assay-ebpf/src/main.rs
crates/assay-ebpf/src/
  open_events.rs
  connect_events.rs
  fork_events.rs
  path_filter.rs
```

Step5 policy-mod split:

```text
crates/assay-core/src/mcp/policy/mod.rs
crates/assay-core/src/mcp/policy/
  types.rs
  deserialize.rs
  matcher.rs
  contracts.rs
```

Existing directories such as `engine_next/*` remain in place. Step5 is about thinning
`mcp/policy/mod.rs`, not redesigning the policy engine.

For files that already exist as `foo.rs`, the split modules live under `foo/*.rs` and are declared
from the facade with `mod child;`. Do not add a sibling `foo/mod.rs`, because that would conflict
with the existing facade module.

## Step Structure

### Step1 - Freeze and gates

Docs and gate only.

Deliverables:

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`

Step1 constraints:

- no edits under `crates/**/src/**/*.rs`
- no edits under `crates/**/tests/**`
- no workflow edits
- no generated-file edits

### Step2 - High-readiness mechanical split

Targets:

- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-core/src/report/summary.rs`
- `crates/assay-cli/src/cli/commands/bundle.rs`

Acceptance:

- facade files delegate to internal modules
- no output or serialization drift
- `lockfile.rs` keeps existing public data types stable
- `summary.rs` keeps summary JSON stable
- `bundle.rs` keeps archive and coverage behavior stable

### Step3 - CLI command split

Targets:

- `crates/assay-cli/src/cli/commands/runner_spike.rs`
- `crates/assay-cli/src/cli/commands/doctor.rs`

Acceptance:

- command enum wiring is unchanged
- exit codes and stdout/stderr behavior are unchanged
- cgroup and kernel-capture behavior are unchanged
- doctor fix preview/apply behavior is unchanged

### Step4 - Runner and eBPF split

Targets:

- `crates/assay-runner-core/src/kernel.rs`
- `crates/assay-ebpf/src/main.rs`

Acceptance:

- kernel event decoding is unchanged
- health-note strings and cgroup correlation behavior are unchanged
- eBPF tracepoint names, map names, and event payloads are unchanged
- no changes to `vmlinux.rs`

### Step5 - Policy facade split and closure

Target:

- `crates/assay-core/src/mcp/policy/mod.rs`

Acceptance:

- YAML/JSON deserialization compatibility is unchanged
- policy decision semantics are unchanged
- reason-code strings are unchanged
- existing `engine_next/*` behavior is untouched
- final review pack lists LOC deltas and residual hotspots

## Required Gates

Every step:

```bash
cargo fmt --check
git diff --check
```

Per-crate gates:

```bash
cargo check -p assay-registry
cargo test -q -p assay-registry

cargo check -p assay-core
cargo test -q -p assay-core --test policy_engine_test
cargo test -q -p assay-core --lib report::summary

cargo check -p assay-cli
cargo test -q -p assay-cli

cargo check -p assay-runner-core
cargo test -q -p assay-runner-core

cargo check -p assay-ebpf
```

Medium and high-risk shared-code steps also run:

```bash
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo clippy -p assay-core --all-targets -- -D warnings
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo clippy -p assay-runner-core --all-targets -- -D warnings
```

The eBPF step must use the crate's existing supported check path. If `cargo check -p assay-ebpf`
requires host/kernel prerequisites that are unavailable on the reviewer machine, the review pack must
show the exact failing prerequisite and the fallback command that was run.

## Reviewer Failure Modes

- changing policy behavior while moving policy types
- changing CLI output or exit codes while splitting command files
- changing summary JSON or bundle archive contents while moving helpers
- widening visibility to make moves compile faster
- touching generated `vmlinux.rs`
- using the one-wave label to batch unrelated cleanup

## Promotion Shape

The preferred stacked PR order is:

1. Step1 freeze and gates
2. Step2 high-readiness split
3. Step3 CLI split
4. Step4 runner/eBPF split
5. Step5 policy facade split and closure

Each step can merge independently after its review script passes. The final closure PR should update
the review pack with actual LOC deltas and any deferred follow-up work.
