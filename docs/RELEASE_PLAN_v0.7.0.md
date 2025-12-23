# v0.7.0 Execution Plan: SDK-First Evaluation

**Goal**: `from verdict_sdk import Evaluator` becomes the standard UX.

## Phase 0: API Contract & DX (P0)
- [ ] **0.1 Evaluator Constructor**
    - [ ] Implement `Evaluator(config=...)` loader logic (`verdict_sdk/evaluator.py`).
    - [ ] Implement `EvaluatorOptions` dataclass.
    - [ ] Verify `Evaluator()` defaults to `eval.yaml` in cwd.

## Phase 1: Result Types & Errors (P0)
- [ ] **1.1 Result Objects**
    - [ ] Create `verdict_sdk/result.py` (`EvalRun`, `CompareResult`, `TestResult`).
    - [ ] Implement helpers: `__bool__`, `raise_for_status()`, `to_github_summary()`.
- [ ] **1.2 Typed Errors**
    - [ ] Create `verdict_sdk/errors.py` (`VerdictError`, `ConfigError`, `RegressionError`).

## Phase 2: Baselines & Compare (P0)
- [ ] **2.1 BaselineStore**
    - [ ] Create `verdict_sdk/baseline.py` handling `.eval/baselines/`.
    - [ ] Implement atomic writes (tmp -> rename).
- [ ] **2.2 API Semantics**
    - [ ] Implement `Evaluator.run()` (Eval logic).
    - [ ] Implement `Evaluator.compare()` (Diff logic).
    - [ ] Implement `Evaluator.save_baseline()`.

## Phase 3: Judge Hardening (P0)
- [ ] **3.1 Prompt Integrity**
    - [ ] Add version/hash validation for prompts.
- [ ] **3.2 Hardened Prompts**
    - [ ] Write `faithfulness_v1.md` and `relevance_v1.md` in `verdict-metrics`.
- [ ] **3.3 Caching**
    - [ ] Implement `JudgeCacheKey` logic (robust hashing).

## Phase 4: Golden Quickstart (P0)
- [ ] **4.1 Quickstart**
    - [ ] Create `examples/evaluator-quickstart/quickstart.py`.
    - [ ] Create minimal `eval.yaml`.
- [ ] **4.2 Migration**
    - [ ] Create `verdict_sdk/compat.py` for CLI config loading.

## Verified By
- `tests/e2e/sdk_evaluator_quickstart.sh`
