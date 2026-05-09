# SPLIT MOVE MAP - Wave 51 Runner Step1

## Intent

Keep `crates/assay-core/src/engine/runner.rs` as the stable facade and move remaining implementation-heavy runner logic into `runner_next`.

## Moves

| From | To | Notes |
| --- | --- | --- |
| `Runner::apply_agent_assertions` body | `runner_next/assertions.rs::apply_agent_assertions_impl` | Preserves final row status/message/details behavior. |
| `Runner::run_test_once` body | `runner_next/single.rs::run_test_once_impl` | Preserves fingerprint, incremental cache, VCR cache, enrichment, metric evaluation, baseline overlay, and trace marker behavior. |
| `runner_next/mod.rs` | adds `assertions` and `single` modules | Keeps implementation modules crate-private. |

## Data Flow

1. `runner_next::execute::run_test_with_policy_impl` still calls `runner.apply_agent_assertions`.
2. `runner_next::execute::run_attempt_with_policy_impl` still calls `runner.run_test_once`.
3. `runner.rs` methods now forward to `runner_next` implementation functions.
4. `runner_next/single.rs` calls existing runner helpers for LLM, semantic enrichment, judge enrichment, and baseline checks.

## Reviewer Focus

- `runner.rs` remains the only public facade for callers.
- No public API or re-export changes.
- Single-test execution order is unchanged.
- Metric span field names remain unchanged.
- The move does not broaden visibility beyond `pub(crate)`.
