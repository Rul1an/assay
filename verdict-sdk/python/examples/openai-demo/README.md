# OpenAI Instrumentor Demo

This example demonstrates how to:
1.  Record an OpenAI chat completion using `verdict_sdk` without writing manual steps.
2.  **New:** Run a "Tool Loop" where the SDK executes tools and feeds results back to the model.
3.  Gate the resulting trace using `verdict ci` with strict replay.

## Prerequisites
- `pip install verdict-sdk` (or use local path)
- `cargo build --release` (for verdict binary)

## Usage

### 1. Simple Trace (Phase 1.1)
Run the script to generate `traces/openai.jsonl`. Set `OPENAI_API_KEY=mock` to avoid actual API calls.

```bash
export PYTHONPATH=../../
export VERDICT_TRACE=traces/openai.jsonl
export OPENAI_API_KEY=mock

python3 ../openai_record.py
```

### 2. Tool Loop Trace (Phase 1.2)
Run the script in loop mode (executes tools, records results).

```bash
export RECORDER_MODE=loop
python3 ../openai_record.py
```

### 3. Verify with Verdict
Run the CI gate. Since we recorded the tool calls, `trace_must_call_tool` should pass.

```bash
# Ensure correct config is used (may depend on exact test ID in yaml)
../../../target/release/verdict ci \
  --config verdict.yaml \
  --trace-file traces/openai.jsonl \
  --db :memory: \
  --replay-strict
```
