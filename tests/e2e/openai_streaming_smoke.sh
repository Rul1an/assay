#!/bin/bash
set -euo pipefail

echo "Build Verdict (Release)..."
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

export PYTHONPATH=$PWD/assay/python

TRACE_FILE="$PWD/assay/python/examples/openai-demo/traces/openai_stream.jsonl"
CONFIG_FILE="$PWD/assay/python/examples/openai-demo/verdict_stream.yaml"

: > "$TRACE_FILE"

echo "Recording Streaming Trace (Mock)..."
export VERDICT_TRACE="$TRACE_FILE"
export RECORDER_MODE=stream
python3 assay/python/examples/openai-demo/record_sync.py

echo "Trace sanity:"
grep '"type":"episode_start"' "$TRACE_FILE" >/dev/null
grep '"type":"tool_call"' "$TRACE_FILE" >/dev/null
grep '"tool_name":"GetWeather"' "$TRACE_FILE" >/dev/null

echo "Running Verdict CI (Replay Strict)..."
$VERDICT ci \
  --config "$CONFIG_FILE" \
  --trace-file "$TRACE_FILE" \
  --db :memory: \
  --replay-strict

echo "✅ OpenAI Streaming Smoke Test Passed"
