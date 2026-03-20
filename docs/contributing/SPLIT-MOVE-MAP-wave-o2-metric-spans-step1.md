# SPLIT MOVE MAP — Wave O2 Metric Spans Step1

## Intent
This step is additive tracing only. There is no facade split and no module move; the runner keeps its current API and evaluation order.

## Data flow map
1. `crates/assay-core/src/engine/runner.rs`
   - creates an `assay.eval.metric` span for each metric in the existing evaluation loop
   - records verdict and duration fields on success
   - records error fields and duration before returning metric evaluation errors
2. `crates/assay-core/src/engine/runner_next/execute.rs`
   - propagates the current tracing subscriber into spawned worker tasks with `.with_current_subscriber()`
3. `crates/assay-core/tests/runner_metric_spans.rs`
   - proves the export contract in a dedicated integration-test process
   - keeps tracing capture isolated from the parallel `--lib` suite

## Reviewer focus
- Span creation stays inside the existing metric loop and does not alter cache or baseline flow
- Spawned tasks inherit the active subscriber, so `run_suite()` actually emits the new spans
- Recorded fields match the review pack exactly and are present on span close
- Error recording happens before the original error is returned
- The integration harness is process-isolated and does not introduce new production dependencies or config knobs
