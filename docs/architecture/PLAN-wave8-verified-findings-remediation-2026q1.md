# PLAN: Wave8 Verified Findings Remediation (Q1 2026)

Status: proposed
Date: 2026-02-18
Verification basis: repository HEAD `34d36e78`

## 1. Context and verified findings

This plan is based on code-verified findings (not speculative findings):

1. Policy hot-path compilation in runtime path:
- `crates/assay-core/src/policy_engine.rs` compiles JSON schema and regex per invocation.
2. Socket CIDR telemetry attribution mismatch:
- `crates/assay-ebpf/src/socket_lsm.rs` emits `action` as `rule_id` for CIDR deny path.
- `crates/assay-monitor/src/loader.rs` writes CIDR map values as action, not rule id.
3. Blocking file I/O inside async runner path:
- `crates/assay-core/src/engine/runner.rs` uses `std::fs::read_to_string` in `async fn run_test_once`.
4. Non-atomic shared eBPF stats updates:
- `crates/assay-ebpf/src/lsm.rs`, `crates/assay-ebpf/src/main.rs`, `crates/assay-ebpf/src/socket_lsm.rs` increment shared map counters with `*ptr += 1`.
5. Ring buffer reserve-drop observability gap:
- eBPF emit paths reserve/submit without explicit drop counters on reserve-fail.

## 2. Program goals and non-goals

Goals:
- Eliminate correctness debt in socket deny attribution.
- Remove avoidable runtime compile cost from policy evaluation hot paths.
- Remove blocking filesystem reads from async execution paths.
- Improve kernel-side observability correctness for stats/drops.
- Keep behavior and public interfaces stable unless explicitly stated.

Non-goals:
- No broad redesign of policy language semantics.
- No unrelated workflow/CI refactors.
- No speculative optimizer work beyond measured bottlenecks above.

## 3. Wave sequence and priority

Execution order:
1. Wave8A: Socket telemetry correctness (highest correctness value).
2. Wave8B: Policy compile caching and compiled-context integration.
3. Wave8C: Async I/O hygiene in runner path.
4. Wave8D: eBPF stats/drop resiliency.

Rationale:
- Fixing wrong `rule_id` attribution first removes misleading audit signals.
- Then reduce known hot-path compute overhead.
- Then address async runtime starvation risk.
- Then harden telemetry/statistics reliability and pressure visibility.

## 4. Wave8A: Socket telemetry correctness

### Intent
Ensure CIDR deny events contain the actual matched policy `rule_id`, not action code.

### Scope lock
Expected touched files:
- `crates/assay-ebpf/src/socket_lsm.rs`
- `crates/assay-monitor/src/loader.rs`
- `crates/assay-policy/src/tiers.rs`
- Docs/reviewer artifacts for this wave only.

### Step 1 (freeze/inventory/gates)
Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave8a-step1-socket-telemetry.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8a-step1-socket-telemetry.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8a-step1-socket-telemetry.md`
- `scripts/ci/review-wave8a-step1.sh`

Required gates:
- Scope allowlist hard-fail.
- Baseline evidence of current mismatch (CIDR uses action value for emitted rule id).
- No production behavior change in Step1.

### Step 2 (mechanical implementation)
Commit slicing:
1. Data shape: define CIDR map value as a typed entry carrying `action` + `rule_id`.
2. Loader mapping: populate entry from compiled policy tiers.
3. Kernel use: emit true `rule_id` while preserving action semantics.

### Step 3 (closure/gates)
Closure gates:
- Hard-fail if CIDR emit path passes action as rule id.
- Hard-fail if loader writes action-only payload into CIDR maps.
- Contract tests for IPv4 and IPv6 deny attribution.

Acceptance:
- Deny events for CIDR rules include stable policy `rule_id`.
- `cargo fmt --check`, clippy on touched crates, and targeted tests pass.

## 5. Wave8B: Policy hot-path compile elimination

### Intent
Move schema/regex compilation out of per-invocation hot paths and into a compiled context/cache.

### Scope lock
Expected touched files:
- `crates/assay-core/src/policy_engine.rs`
- `crates/assay-core/src/validate/mod.rs`
- `crates/assay-core/src/agent_assertions/matchers.rs`
- optional new internal context module under policy engine.

### Step 1 (freeze/inventory/gates)
Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave8b-step1-policy-hotpath.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8b-step1-policy-hotpath.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step1-policy-hotpath.md`
- `scripts/ci/review-wave8b-step1.sh`

Required gates:
- Record all current `validator_for` and `Regex::new` callsites in policy-eval paths.
- Define acceptance latency metrics to compare before/after.

### Step 2 (mechanical implementation)
Commit slicing:
1. Introduce `CompiledPolicyContext` (schema and regex cache/compiled artifacts).
2. Add adapters for current callsites without semantic changes.
3. Wire runtime callsites to compiled context.

### Step 3 (closure/gates)
Closure gates:
- Hard-fail if policy hot paths call `jsonschema::validator_for` directly.
- Hard-fail if sequence hot paths call `regex::Regex::new` directly.
- Verify output parity on existing policy/assertion tests.

Acceptance:
- Functional parity maintained.
- Hot-path compile calls removed from runtime evaluation paths.

## 6. Wave8C: Async runner I/O hygiene

### Intent
Remove blocking file I/O from async runner execution path.

### Scope lock
Expected touched files:
- `crates/assay-core/src/engine/runner.rs`
- optional prep/resolution path if policy hash pre-read is moved earlier.

### Step 1 (freeze/inventory/gates)
Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave8c-step1-async-io.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8c-step1-async-io.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8c-step1-async-io.md`
- `scripts/ci/review-wave8c-step1.sh`

Required gates:
- Identify all blocking fs calls in async runner path.
- Define invariant: fingerprint/cache behavior remains unchanged.

### Step 2 (mechanical implementation)
Commit slicing:
1. Move policy hash read to pre-resolve/preload stage, or async-safe boundary.
2. Keep `run_test_once` behavior identical from caller perspective.
3. Add tests proving fingerprint equivalence for same inputs.

### Step 3 (closure/gates)
Closure gates:
- Hard-fail on `std::fs::read_to_string` in async runner method bodies.
- Parity tests for fingerprint and skip-cache semantics.

Acceptance:
- No blocking fs reads in async run path.
- Cache/fingerprint semantics preserved.

## 7. Wave8D: eBPF stats and drop resiliency

### Intent
Reduce stats undercount risk and expose ringbuf pressure drops explicitly.

### Scope lock
Expected touched files:
- `crates/assay-ebpf/src/lsm.rs`
- `crates/assay-ebpf/src/main.rs`
- `crates/assay-ebpf/src/socket_lsm.rs`
- userspace stats aggregation/readout in monitor crate if needed.

### Step 1 (freeze/inventory/gates)
Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave8d-step1-ebpf-stats.md`
- `docs/contributing/SPLIT-CHECKLIST-wave8d-step1-ebpf-stats.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8d-step1-ebpf-stats.md`
- `scripts/ci/review-wave8d-step1.sh`

Required gates:
- Baseline list of non-atomic increments and reserve-drop paths.
- Define final counter model (per-CPU or verifier-safe atomic approach).

### Step 2 (mechanical implementation)
Commit slicing:
1. Counter model migration for stats increments.
2. Drop-counter additions on ringbuf reserve-fail paths.
3. Userspace readout/aggregation updates and tests.

### Step 3 (closure/gates)
Closure gates:
- Hard-fail on raw shared `*ptr += 1` for targeted stats maps.
- Hard-fail if emit reserve-fail paths do not update drop signal.

Acceptance:
- Stats behavior is stable under concurrency tests.
- Drop visibility is present in telemetry outputs.

## 8. Branch and PR strategy

Per wave/step branch naming:
- `codex/wave8a-step1-socket-telemetry-freeze`
- `codex/wave8a-step2-socket-telemetry-fix`
- `codex/wave8a-step3-socket-telemetry-close`
- Repeat equivalent pattern for `wave8b`, `wave8c`, `wave8d`.

PR structure:
- Keep A/B/C slices small and reviewable.
- Enable auto-merge (squash) only after required checks are green.
- For stacked work, always create explicit promotion PR to `main`.

## 9. Validation baseline for each step

Minimum validation commands:
- `cargo fmt --check`
- `cargo clippy -p <touched-crate> --all-targets -- -D warnings`
- `cargo check -p <touched-crate>`
- Wave reviewer script: `BASE_REF=origin/main bash scripts/ci/review-wave8*-step*.sh`

When kernel/eBPF paths are changed:
- Include current eBPF smoke and monitor checks that already exist in CI.

## 10. Definition of done (program level)

Wave8 is complete when:
1. Wave8A-8D Step1/2/3 are merged to `main`.
2. All closure scripts pass against `origin/main`.
3. Roadmap/program docs are synchronized with merged outcomes.
4. A final review pack summarizes:
- evidence of correctness fixes,
- performance/runtime-health effects,
- residual risk and deferred follow-ups.
