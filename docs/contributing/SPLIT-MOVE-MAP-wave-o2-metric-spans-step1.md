# SPLIT MOVE MAP — Wave O2 Metric Spans Step1

## Intent
This step is additive tracing only. There is no facade split and no module move; the runner keeps its current API and evaluation order.

## Data flow map
1. `crates/assay-core/src/engine/runner.rs`
   - creates an `assay.eval.metric` span for each metric in the existing evaluation loop
   - records verdict and duration fields on success
   - records error fields and duration before returning metric evaluation errors
   - proves the export contract with runner-local tracing capture tests

## Reviewer focus
- Span creation stays inside the existing metric loop and does not alter cache or baseline flow
- Recorded fields match the review pack exactly and are present on span close
- Error recording happens before the original error is returned
- The test harness is local to the runner and does not introduce new production dependencies or config knobs
