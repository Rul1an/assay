#!/bin/bash
set -euo pipefail

# Build Verdict
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

# Ensure PYTHONPATH sees the SDK
export PYTHONPATH=$PWD/assay/python

TRACE_FILE="$PWD/assay/python/examples/openai-demo/traces/openai.jsonl"
CONFIG_FILE="$PWD/assay/python/examples/openai-demo/verdict.yaml"

# Truncate trace file for determinism
: > "$TRACE_FILE"

# 1. Run Recording (Mock Mode)
echo "Recording Trace..."
export VERDICT_TRACE="$TRACE_FILE"
export OPENAI_API_KEY="mock"

python3 assay/python/examples/openai-demo/record_sync.py

echo "Trace Content:"
cat "$TRACE_FILE"

# 2. Run Verdict CI (Replay Strict)
echo "Running Verdict CI..."
$VERDICT ci \
  --config "$CONFIG_FILE" \
  --trace-file "$TRACE_FILE" \
  --db :memory: \
  --replay-strict

echo "✅ OpenAI SDK Smoke Test Passed"
