# SPLIT CHECKLIST — Wave O2 Metric Spans Step1

## Scope discipline
- [ ] Only allowlisted files changed for this step
- [ ] No `.github/workflows/*` changes
- [ ] No `assay-monitor`, `assay-ebpf`, registry, Python SDK, fuzz, or release-workflow changes
- [ ] No new CLI flags or config fields
- [ ] No non-metric runner behavior changes
- [ ] Metric span contract coverage lives in an isolated integration test process

## Contract checks
- [ ] Every metric evaluation in `Runner::run_test_once()` is wrapped in an `assay.eval.metric` span
- [ ] `run_suite()` propagates the active subscriber into spawned worker tasks
- [ ] Success spans record metric name, cached bit, score, pass/fail state, unstable bit, and duration
- [ ] Error spans record `error=true`, `error.message`, and duration before bubbling the error
- [ ] Existing runner verdict semantics remain unchanged
- [ ] Existing LLM tracing contract stays out of scope and remains green

## Non-goals
- [ ] No ring-buffer telemetry edits in this step
- [ ] No rule-by-rule policy spans in this step
- [ ] No workflow, release, or packaging changes in this step

## Validation
- [ ] `BASE_REF=origin/codex/codebase-analysis-observability bash scripts/ci/review-wave-o2-metric-spans-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core --all-targets -- -D warnings` passes
- [ ] `cargo check -p assay-core` passes
- [ ] `cargo test -p assay-core --lib` passes
- [ ] `cargo test -p assay-core --test runner_metric_spans` passes
- [ ] `cargo test -p assay-core --test otel_contract` passes
- [ ] `git diff --check` passes
