# Wave49 Step2 Move Map

Facade retained in `crates/assay-sim/src/attacks/memory_poison.rs`:

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

Moved to `crates/assay-sim/src/attacks/memory_poison_next/basis.rs`:

- `make_clean_deny_basis`
- `make_clean_allow_basis`
- `compute_snapshot_id`
- `condition_b_replay_integrity`
- `compute_basis_hash`

Moved to `crates/assay-sim/src/attacks/memory_poison_next/vectors.rs`:

- Condition A vectors V1-V4
- replay baseline poisoning path
- deny convergence poisoning path
- context-envelope poisoning path
- decay/snapshot poisoning path

Moved to `crates/assay-sim/src/attacks/memory_poison_next/controls.rs`:

- benign controls B1-B3
- no-false-positive control evaluation paths

Moved to `crates/assay-sim/src/attacks/memory_poison_next/conditions.rs`:

- `vector1_condition_b`
- `vector2_condition_b`
- `vector4_condition_b`
- `vector3_condition_c`

Moved to `crates/assay-sim/src/attacks/memory_poison_next/matrix.rs`:

- `make_result`
- matrix runner loop from `run_memory_poison_matrix`
- condition/result assembly and naming logic

Explicitly unchanged in this wave:

- `crates/assay-sim/tests/**`
- `crates/assay-core/**`
- `crates/assay-cli/**`
- `crates/assay-evidence/**`
- `.github/workflows/**`
