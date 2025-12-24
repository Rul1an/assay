# OpenAI Instrumentor Demo

This example demonstrates how to:
1.  Record an OpenAI chat completion using `assay` without writing manual steps.
2.  **New:** Run a "Tool Loop" where the SDK executes tools and feeds results back to the model.
3.  Gate the resulting trace using `assay ci` with strict replay.

## Prerequisites
- `pip install assay` (or use local path)
- `cargo build --release` (for assay binary)

## Usage

### 1. Simple Trace (Phase 1.1)
Run the script to generate `traces/openai.jsonl`. Set `OPENAI_API_KEY=mock` to avoid actual API calls.

```bash
export PYTHONPATH=../../
export VERDICT_TRACE=traces/openai.jsonl
export OPENAI_API_KEY=mock

python3 record_sync.py
```

### 2. Tool Loop Trace (Phase 1.2)
Run the script in loop mode (executes tools, records results).

```bash
export RECORDER_MODE=loop
python3 record_sync.py
```

### 3. Streaming Trace
Run in streaming mode.

```bash
export RECORDER_MODE=stream
python3 record_sync.py
```

### 4. Async Usage (Phase 1.4)
Run the async version of the recorder. Supports `simple`, `loop`, and `stream` modes via `RECORDER_MODE`.

```bash
python3 record_async.py
```

### 5. Verify with Assay
Run the CI gate.

```bash
../../../target/release/assay ci \
  --config assay.yaml \
  --trace-file traces/openai.jsonl \
  --db :memory: \
  --replay-strict
```
