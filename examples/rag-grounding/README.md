# RAG Grounding Demo

This demo showcases how Verdict detects **hallucinations** and **semantic regressions** in a RAG pipeline.
It runs entirely offline using **Deterministic Replay Mode**.

## Quickstart

### 1. The "Good" Run (Pass) ✅
The implementation correctly answers the question about deductibles (€385) and cites sources.
```bash
target/release/verdict ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/good.jsonl \
  --baseline examples/rag-grounding/baseline.json \
  --strict
```

### 2. The "Bad" Run (Fail) ❌
The implementation hallucinates the amount (€500) and adds an unsupported claim.
Verdict catches this via:
- `must_not_contain`: "€500" found.
- `semantic_similarity`: Score drops below baseline threshold (`max_drop`).

```bash
target/release/verdict ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/hallucination.jsonl \
  --baseline examples/rag-grounding/baseline.json \
  --strict
```

## What's happening?
- **Replay Mode**: We use `--trace-file` to feed pre-recorded LLM outputs into Verdict. No API keys required.
- **Baselines**: We compare the run-time score against `baseline.json`. If the score drops significantly, the build fails.
- **Redaction**: PII/Sensitive data in prompts is redacted in `run.json` by default.
