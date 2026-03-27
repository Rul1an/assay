# Wave49 Step3 Move Map (Closure)

Stable facade retained in `crates/assay-sim/src/attacks/memory_poison.rs`:

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
- `#[path = "memory_poison_next/mod.rs"] mod memory_poison_next;`

Shipped implementation ownership in `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`:

- clean replay-diff basis builders
- snapshot-hash helpers
- fail-closed replay basis hashing path from `#975`

Shipped implementation ownership in `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`:

- Condition A vectors V1-V4
- replay baseline poisoning path
- deny convergence poisoning path
- context-envelope poisoning path
- decay/snapshot poisoning path

Shipped implementation ownership in `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`:

- benign controls B1-B3
- no-false-positive control evaluation paths

Shipped implementation ownership in `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`:

- Condition B/C defense paths for the applicable vectors

Shipped implementation ownership in `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`:

- result builder
- matrix runner loop
- condition/result assembly and naming logic

Future cleanup still out of scope in Step3:

- future internal visibility tightening only if it requires a separate code wave
- memory-poison result or attack-name contract changes
- replay/context semantics changes
- `crates/assay-sim/tests/**`
- `crates/assay-core/**`
- `crates/assay-cli/**`
- `crates/assay-evidence/**`
- `.github/workflows/**`
