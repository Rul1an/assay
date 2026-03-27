# Wave49 Step1 Move Map — `sim/attacks/memory_poison.rs`

## Stable facade to preserve in Step2

The following surface remains owned by `crates/assay-sim/src/attacks/memory_poison.rs` in Step1
and must keep the same meaning in Step2:
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
- existing inline tests

## Proposed Step2 ownership split

- `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`
  - clean replay-diff basis builders
  - snapshot/hash helper construction inputs
- `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`
  - Condition A attack vectors V1-V4
- `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`
  - benign controls B1-B3
- `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`
  - Condition B/C detection helpers and vector wrappers
- `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`
  - result builder and matrix runner assembly

## Pinned invariants

- identical replay-diff bucket behavior
- identical Condition B/C detection behavior
- identical benign-control no-false-positive behavior
- identical `PoisonOutcome` / `AttackStatus` mapping
- identical matrix cardinality and attack-name formatting
- identical integration-test expectations in `memory_poison_invariant`

## Explicitly unchanged in this wave

- `crates/assay-sim/tests/**`
- `crates/assay-core/**`
- `crates/assay-cli/**`
- `crates/assay-evidence/**`
- `.github/workflows/**`
