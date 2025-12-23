#!/bin/bash
set -euo pipefail

# Build Verdict
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

export PYTHONPATH=$PWD/verdict-sdk/python
TRACE_FILE="$PWD/verdict-sdk/python/examples/openai-demo/traces/openai.jsonl"
CONFIG_FILE="$PWD/verdict-sdk/python/examples/openai-demo/verdict.yaml"

# Truncate
: > "$TRACE_FILE"

# 1. Run Recording (Loop Mode)
echo "Recording Trace (Loop)..."
export VERDICT_TRACE="$TRACE_FILE"
export OPENAI_API_KEY="mock"
export RECORDER_MODE="loop"

python3 verdict-sdk/python/examples/openai_record.py

echo "Trace Content (grep result):"
grep '"result":' "$TRACE_FILE" || { echo "❌ Failed: No tool result recorded"; exit 1; }

# 2. Run Verdict CI (Replay Strict)
# Note: config expects "openai_weather_demo", but loop uses "openai_loop_demo".
# We need to update verdict.yaml or config overrides?
# Actually, the user spec implies we use a generic config.
# But let's check what ID is used in record script: "openai_loop_demo".
# We need to ensure config has a test for that ID or a test that matches.
# For smoke test, let's append a test definition to verdict.yaml temporarily or use a new one.
# Simpler: Modify the record script to reuse "openai_weather_demo" ID?
# No, different logic.
# Let's create a temporary config for loop.

LOOP_CONFIG="$PWD/verdict-sdk/python/examples/openai-demo/verdict_loop.yaml"
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
