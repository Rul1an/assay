# Strategic Roadmap: v0.7.0 & Beyond

**Current State**: v0.6.0 "Boring Infra" (SDK Integration Parity)
**Target**: v0.7.0 "SDK-First Evaluation"

## 🧭 The Strategic Pivot

Based on the v0.6.0 retrospective, we are pivoting the v0.7.0 focus from **CLI Ergonomics** to **SDK-Native Evaluation**.

**Rationale**: The SDK has proven to be the superior integration point (68% LoC reduction). Users should not be forced to context-switch to a CLI to run evaluations. The entire loop (Record -> Eval -> Assert) should act as a standard Python API.

---

## 1. SDK-Native Evaluation (The "Evaluator" Class)

**Goal**: Allow running evaluations programmatically within tests (pytest/unittest).

```python
from verdict_sdk import Evaluator

def test_rag_agent():
    # 1. Record
    trace = agent.run("query")

    # 2. Evaluate (New)
    evaluator = Evaluator(config="eval.yaml", baseline="baseline.json")
    result = evaluator.compare(trace)

    # 3. Assert (Standard Python)
    assert result.passed, f"Regression: {result.diff}"
    assert result.metrics["faithfulness"] > 0.9
```

**Work Required**:
*   Expose `verdict-core` logic to Python (via PyO3 binding or subprocess wrapper initially).
*   Define `Evaluator` API contract.

---

## 2. Answers to Leadership Questions

### Q: Judge Metrics Status?
*   **Must Contain / Regex**: ✅ Stable.
*   **Semantic**: ✅ Stable (Embedding variants).
*   **Faithfulness / Relevance**: 🚧 Wired in Rust (`verdict-metrics/src/lib.rs`), but relies on simplistic prompters. Needs "Golden Prompt" hardening.
*   **Multi-sample Voting**: ✅ **Implemented**. The `JudgeService` in Rust already supports `samples: N` and calculates `agreement` score.

### Q: Baseline Management in SDK?
This is the core of v0.7.0. The `Evaluator.compare()` method will handle the diff logic, making baselines "just another JSON file" you check into git, rather than a CLI flag mystery.

### Q: "15 Minutes to Green"?
**Current bottleneck**: Configuring the `eval.yaml` rules.
**Fix**: v0.7.0 should include "Auto-Config" presets (e.g. `evaluator = Evaluator.preset("rag_safety")`).

---

## 3. Marketing Assets

### The "68% Reduction" Pitch
**Before (v0.2.0 Manual)**:
*   Imports: 10+
*   Lines: 530
*   Complexity: Custom `Episode` class, manual `while` loop, manual JSON serialization.

**After (v0.6.0 SDK)**:
*   Imports: 3
*   Lines: 170
*   Complexity: Single `record_chat_completions_with_tools` call.

---

## 4. Next Steps (v0.7.0 Roadmap)

1.  **SDK**: Design `Evaluator` class interface (RFC).
2.  **Core**: Harden `Faithfulness` prompts (Evaluation Engineering).
3.  **Docs**: Create "SDK Evaluation" guide.
