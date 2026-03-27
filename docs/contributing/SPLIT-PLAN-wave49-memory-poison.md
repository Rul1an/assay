# Wave49 Plan — `sim/attacks/memory_poison.rs` Kernel Split

## Goal

Split `crates/assay-sim/src/attacks/memory_poison.rs` behind a stable facade with zero
memory-poison semantic drift and no downstream replay, context-envelope, or report-output drift.

Current hotspot baseline on `origin/main @ b2a83c58`:
- `crates/assay-sim/src/attacks/memory_poison.rs`: `954` LOC
- `crates/assay-sim/tests/memory_poison_invariant.rs`: memory-poison matrix companion
- `crates/assay-core/src/mcp/decision.rs`: replay-diff / context contract companion

## Step1 (freeze)

Branch: `codex/wave49-memory-poison-step1` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave49-memory-poison.md`
- `docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step1.md`
- `scripts/ci/review-wave49-memory-poison-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-sim/src/attacks/**`
- no edits under `crates/assay-sim/tests/**`
- no workflow edits
- no `assay-core`, `assay-cli`, `assay-evidence`, or report-surface edits

Step1 gate:
- allowlist-only diff (the 5 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-sim/src/attacks/**`
- hard fail on untracked files in `crates/assay-sim/src/attacks/**`
- hard fail on tracked changes in `crates/assay-sim/tests/**`
- hard fail on untracked files in `crates/assay-sim/tests/**`
- `cargo fmt --check`
- `cargo clippy -p assay-sim --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::vector1_activates_under_condition_a' -- --exact`
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::vector3_activates_under_condition_a' -- --exact`
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::controls_produce_no_false_positives' -- --exact`
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::full_matrix_runs_without_panic' -- --exact`
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::condition_b_blocks_v1_and_v2' -- --exact`
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::condition_c_blocks_v3' -- --exact`
  - `cargo test -q -p assay-sim --lib 'attacks::memory_poison::tests::overarching_invariant_controls_never_misclassify' -- --exact`
  - `cargo test -q -p assay-sim --test memory_poison_invariant overarching_invariant_no_silent_downgrades_in_controls -- --exact`
  - `cargo test -q -p assay-sim --test memory_poison_invariant attack_vectors_activate_under_condition_a -- --exact`
  - `cargo test -q -p assay-sim --test memory_poison_invariant condition_b_blocks_replay_vectors -- --exact`
  - `cargo test -q -p assay-sim --test memory_poison_invariant condition_c_blocks_context_envelope -- --exact`
  - `cargo test -q -p assay-sim --test memory_poison_invariant full_matrix_structure -- --exact`

## Frozen public surface

Wave49 freezes the expectation that Step2 keeps these memory-poison entrypoints and outputs
unchanged in meaning:
- `PoisonResult`
- `PoisonOutcome`
- `vector1_replay_baseline_poisoning`
- `vector2_deny_convergence_poisoning`
- `vector3_context_envelope_poisoning`
- `vector4_decay_escape`
- `control_b1_run_metadata_recall`
- `control_b2_tool_observation_recall`
- `control_b3_approval_context_recall`
- `run_memory_poison_matrix`

Step2 may reorganize internal ownership behind `memory_poison.rs`, but must not redefine:
- replay-diff bucket behavior for Condition A/B paths
- context-envelope completeness behavior for Condition A/C paths
- snapshot-hash behavior for vector 4
- benign-control no-false-positive behavior
- `PoisonOutcome` meaning or `AttackStatus` mapping
- matrix result count, ordering intent, or attack naming format
- downstream invariant-test expectations

## Status

- Wave48 closed on `main` via `#971`.
- Wave49 Step1 shipped on `main` via `#972`.
- Step2 is the mechanical split slice for `memory_poison.rs`.

## Step2 (mechanical split preview)

Branch: `codex/wave49-memory-poison-step2` (base: `main`)

Target layout:
- `crates/assay-sim/src/attacks/memory_poison.rs` (thin facade + stable routing)
- `crates/assay-sim/src/attacks/memory_poison_next/mod.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`

Step2 principles:
- 1:1 body moves
- stable `memory_poison.rs` facade behavior
- no replay-diff or context-envelope drift
- no `PoisonOutcome` / `AttackStatus` mapping drift
- no matrix count or attack-name drift
- no edits under `crates/assay-sim/tests/**`
- no workflow edits

Step2 scope:
- `crates/assay-sim/src/attacks/memory_poison.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/mod.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`
- `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`
- `docs/contributing/SPLIT-PLAN-wave49-memory-poison.md`
- `docs/contributing/SPLIT-CHECKLIST-wave49-memory-poison-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave49-memory-poison-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave49-memory-poison-step2.md`
- `scripts/ci/review-wave49-memory-poison-step2.sh`

Current Step2 shape:
- `memory_poison.rs`: stable facade, public result/outcome surface, wrappers, and existing inline tests
- `memory_poison_next/basis.rs`: clean replay-diff basis builders and hash helpers
- `memory_poison_next/vectors.rs`: Condition A vectors V1-V4
- `memory_poison_next/controls.rs`: benign controls B1-B3
- `memory_poison_next/conditions.rs`: Condition B/C defense paths
- `memory_poison_next/matrix.rs`: result builder and matrix runner assembly

Current Step2 LOC snapshot on this branch:
- `crates/assay-sim/src/attacks/memory_poison.rs`: `954 -> 177`
- `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`: `265`
- `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`: `166`
- `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`: `141`
- `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`: `135`
- `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`: `94`

## Step3 (closure)

Step3 will close the shipped Wave49 memory-poison split with docs/gates only once Step2 lands on
`main`.

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once the chain is clean.

## Reviewer notes

This wave must remain memory-poison split planning only.

Primary failure modes:
- sneaking attack-semantic cleanup into a mechanical split
- changing result counts or attack names while chasing file size
- changing Condition B/C detection semantics under a refactor label
- drifting integration-test expectations via helper moves
