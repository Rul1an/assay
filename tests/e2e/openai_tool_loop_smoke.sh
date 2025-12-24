#!/bin/bash
set -euo pipefail

# Build Verdict
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

export PYTHONPATH=$PWD/assay/python
TRACE_FILE="$PWD/assay/python/examples/openai-demo/traces/openai.jsonl"
CONFIG_FILE="$PWD/assay/python/examples/openai-demo/verdict.yaml"

# Truncate
: > "$TRACE_FILE"

# 1. Run Recording (Loop Mode)
echo "Recording Trace (Loop)..."
export VERDICT_TRACE="$TRACE_FILE"
export OPENAI_API_KEY="mock"
export RECORDER_MODE="loop"

python3 assay/python/examples/openai-demo/record_sync.py

echo "Trace Content (grep result):"
grep '"result":' "$TRACE_FILE" || { echo "❌ Failed: No tool result recorded"; exit 1; }
grep '"tool_call_id":' "$TRACE_FILE" || { echo "❌ Failed: No tool_call_id recorded in meta"; exit 1; }

# 2. Run Verdict CI (Replay Strict)
# Use a temporary config tailored for the loop demo.

LOOP_CONFIG="$PWD/assay/python/examples/openai-demo/verdict_loop.yaml"
cat > "$LOOP_CONFIG" <<EOF
version: 1
suite: openai-loop
model: "trace"
tests:
  - id: openai_loop_demo
    input:
      prompt: "What's the weather like in Tokyo?"
    expected:
      type: regex_match
      pattern: ".*"
    assertions:
      - type: trace_must_call_tool
        tool: GetWeather
EOF

echo "Running Verdict CI..."
$VERDICT ci \
  --config "$LOOP_CONFIG" \
  --trace-file "$TRACE_FILE" \
  --db :memory: \
  --replay-strict

echo "✅ OpenAI Tool Loop Smoke Test Passed"
