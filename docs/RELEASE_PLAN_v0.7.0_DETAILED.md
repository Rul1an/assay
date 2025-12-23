# v0.7.0 Detailed Execution Plan (Phase 3+)

## Phase 3.0: Trace Ingest & Matching (P0)
- [x] **3.0.1 Trace Reader**
    - [x] Create `verdict_sdk/trace_reader.py`.
    - [x] Implement `read_events(path) -> list[dict]`.
    - [x] Implement `group_by_episode(events) -> dict`.
    - [x] Helpers: `extract_last_model_content`, `extract_tool_calls`.
- [x] **3.0.2 Match Logic**
    - [x] Modify `Evaluator.run()` to use `TraceReader`.
    - [ ] Implement matching: `meta.test_id` or prompt fallback.
    - [ ] Add `strict` mode logic.

## Phase 3.1: Builtin Metrics Engine (P0)
- [x] **3.1.1 Metrics Module**
    - [x] Create `verdict_sdk/metrics/builtin.py`.
    - [x] Implement `eval_regex_match`.
    - [x] Implement `eval_trace_must_call_tool`.
- [x] **3.1.2 Integration**
    - [x] Wire `Evaluator` to dispatch builtin metrics.
    - [x] Verify `TestResult.passed` aggregation.

## Phase 3.2: Compare & Regressions (P0)
- [x] **3.2.1 Compare Logic**
    - [x] Implement `_index_run` and `_metric_spec_map` helpers.
    - [x] Implement `_passes_threshold` operator logic.
    - [x] Implement `Evaluator.compare()` full logic (Regressions, Thresholds).
- [x] **3.2.2 Diff Writer**
    - [x] Update `RunWriter.write_diff` (Already done in Phase 2).

## Phase 3.3: Judge MVP (P1)
- [ ] **3.3.1 Types & Config**
    - [ ] Create `verdict_sdk/judge/types.py`.
    - [ ] Update `verdict_sdk/config.py` with `JudgeConfig`.
- [ ] **3.3.2 Implementation**
    - [ ] Create `verdict_sdk/judge/openai_judge.py` (Strict JSON).
    - [ ] Create `verdict_sdk/judge/cache.py` (Robust Keying).
- [ ] **3.3.3 Integration**
    - [ ] Wire `Evaluator` to use `JudgeConfig`.
    - [ ] Implement `Evaluator.run` loop dispatch with correct `MetricResult` mapping.

## Phase 3.4: artifacts & CI (P1)
- [ ] **3.4.1 JUnit**
    - [ ] `verdict_sdk/reporting/junit.py`.
- [ ] **3.4.2 DX**
    - [ ] Validated Contracts (Idempotency).
