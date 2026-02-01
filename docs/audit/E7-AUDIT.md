# Audit Evidence Pack: Epic E7 - Judge Reliability (SOTA 2026)

## 1. Scope & Threat Model
This document outlines the reliability controls for the Assay "Judge" module.

**Threats Mitigated:**
| Threat | Mitigation | Implementation |
|--------|------------|----------------|
| **Non-Determinism** | Parallel-safe soft budgets, Shared Atomic state, Deterministic Mocks | `contract_determinism_parallel_replay` |
| **Label Bias** | Blind Labeling (Source Hiding for Absolute, X/Y for Pairwise) | `JudgeService::build_prompt` |
| **Prompt Hijack** | Delimiters (`<input>`) + System Guards | `build_prompt` (Line 85+) |
| **Output Drift** | Robust JSON Parsing (Greedy + Preamble Skip) | `JudgeCallResult::from_json` |
| **Cost Runaway** | Max calls per test (Hard) + Global Soft Limit (Telemetry) | `ReliabilityConfig`, `global_extra_calls` |

## 2. Determinism Contract
We guarantee **Orchestration Determinism**:
> Given the same `seed`, `config`, and `input`, the Judge orchestrator will execute the exact same sequence of LLM calls, produce the same cache keys, and handle retries identically, regardless of thread scheduling or global contention.

**Implementation Details:**
- **Shared State:** `global_extra_calls` is an `AtomicU32` shared across all parallel judge instances.
- **Soft Budget:** Exceeding the global budget triggers a warning (Telemetry) but does *not* abort the test, ensuring the verdict remains `Pass/Fail` based solely on the test's own merit.
- **Cache Key:** Includes a canonical JSON fingerprint of `ReliabilityConfig` to prevent drift.

## 3. Rerun Strategy (Adaptive Majority)
We use an **Adaptive Majority Early-Stop** (formerly SPRT-inspired) strategy:
1.  **Fast Path:** If the first call is confident (score outside `[0.4, 0.6]`), stop.
2.  **Rerun:** If borderline, trigger up to `N` extra calls.
3.  **Aggregation:** Final verdict is based on majority/ratio of votes (agreement).

## 4. Output Contract & Robustness
The Judge output parser is hardened against common LLM verbosity:
- **Preamble Skip:** explicitly seeks `{` or `[` to ignore "Here is your JSON:" chatter.
- **Tolerant Deserializer:** uses `serde_json::Deserializer` to stop parsing after the first valid object, ignoring trailing commentary.

## 5. Audit Evidence: Parallel Determinism
The critical contract test `contract_determinism_parallel_replay` proves that even when the global rate limit is "saturated" (inflated count), two parallel executions of the same test yield:
- **Identical Verdicts** (`Pass`)
- **Identical Metadata** (Score, Extra Calls)
- **Zero Interference** from scheduling order.

## 6. Schema Snippet (Judge Meta)
```json
{
  "assay": {
    "judge": {
      "correctness": {
        "verdict": "Pass",
        "score": 1.0,
        "rubric_version": "v1",
        "votes": [true, true, false],
        "agreement": 0.66,
        "extra_calls_used": 2,
        "cached_at": "2026-02-02T10:00:00Z",
        "config_fingerprint": "{\"rerun_strategy\":\"always_three\"...}"
      }
    }
  }
}
```
