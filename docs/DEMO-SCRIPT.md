# Assay 3-Minute Demo Script

## 0. Setup (15s)
Build the release binary to ensure speed and readiness.
```bash
cargo build --release
```

## 1. “Green / Grounded” Run (45s)
Run the check against the "good" trace. This strictly validates standard behavior.
```bash
target/release/assay ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/good.jsonl \
  --baseline examples/rag-grounding/baseline.json \
  --strict
```
**Talking Points:**
*   “This runs entirely offline (Replay Mode) - 100% deterministic.”
*   “We verify both content (`semantic_similarity`) and safety guards (`must_contain` / `must_not_contain`).”
*   “The Baseline ensures scores don't silently drop (regression testing).”

## 2. “Red / Hallucination” Run (45s)
Run the check against the "bad" trace where the model hallucinates a deductible amount.
```bash
target/release/assay ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/hallucination.jsonl \
  --baseline examples/rag-grounding/baseline.json \
  --strict
```
**Talking Points:**
*   “See exactly which check fails: the amount is hallucinated (`€500` instead of `€385`).”
*   “Notice the semantic score also drops, triggering the baseline regression gate.”

## 3. Inspect Artifacts (60s)
Show the JSON output to demonstrate programmatically actionable results.
```bash
cat run.json | head -n 40
```
**Talking Points:**
*   Highlight `status`, `message`, and `details.metrics`.
*   Explain how this enables automated dashboards/reporting.

## 4. “Baseline Export” (15s)
Demonstrate how easy it is to update the "golden" state.
```bash
target/release/assay ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/good.jsonl \
  --export-baseline examples/rag-grounding/baseline.json \
  --strict
```
**Talking Points:**
*   “Creating or updating a baseline is just one command.”
*   “In CI (on `main`), we export this and upload it as an artifact or commit it.”
