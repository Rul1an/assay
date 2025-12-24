#!/bin/bash
set -euo pipefail

# Build Verdict
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

export PYTHONPATH=$PWD/assay/python
TRACE_FILE="$PWD/assay/python/examples/openai-demo/traces/openai_async.jsonl"

# 1. Generate Async Trace (Loop Mode)
echo "Generating Async Trace..."
export VERDICT_TRACE="$TRACE_FILE"
export OPENAI_API_KEY="mock"
export RECORDER_MODE="loop"

# Truncate trace file
: > "$TRACE_FILE"

python3 assay/python/examples/openai-demo/record_async.py

# 2. Verify with Verdict
# We can mistakenly reuse the loop_idemp config if the test_id matches
# Or create a dedicated one.
# record_async.py uses `test_id="async_loop_demo"`

CONFIG_FILE="$PWD/assay/python/examples/openai-demo/verdict_async.yaml"
cat > "$CONFIG_FILE" <<EOF
version: 1
suite: async-loop-smoke
model: "trace"
tests:
  - id: async_loop_demo
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
$VERDICT ci --config "$CONFIG_FILE" --trace-file "$TRACE_FILE" --db :memory: --replay-strict

echo "✅ Async OpenAI Smoke Test Passed"
