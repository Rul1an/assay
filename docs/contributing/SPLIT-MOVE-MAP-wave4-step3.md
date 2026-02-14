# Wave4 Step3 move map (`explain.rs` mechanical split)

Scope:
- `crates/assay-core/src/explain.rs`
- `crates/assay-core/src/explain_next/*`

No public symbol path changes intended (`crate::explain::*` remains stable).

## Type/public model moves

- `ExplainedStep` -> `explain_next/model.rs`
- `StepVerdict` -> `explain_next/model.rs`
- `RuleEvaluation` -> `explain_next/model.rs`
- `TraceExplanation` -> `explain_next/model.rs`
- `ToolCall` -> `explain_next/model.rs`
- `TraceExplainer` -> `explain_next/source.rs`

## Logic moves

- `TraceExplainer::{explain,explain_step,check_static_constraints,is_alias_member}` -> `explain_next/source.rs`
- `ExplainerState` + rule/state machine (`evaluate_rule`, `update`, `check_end_of_trace`, `snapshot`) -> `explain_next/diff.rs`
- `TraceExplanation::{to_terminal,to_markdown,to_html}` + `summarize_args` -> `explain_next/render.rs`

## Facade contract

`crates/assay-core/src/explain.rs` is now a thin facade:
- `#[path = "explain_next/mod.rs"] mod explain_next;`
- `pub use explain_next::{...};`

No behavior/perf changes intended in this step.
