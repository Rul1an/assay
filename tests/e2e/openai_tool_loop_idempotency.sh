#!/bin/bash
set -euo pipefail

# Build Verdict
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

export PYTHONPATH=$PWD/verdict-sdk/python
TRACE_FILE="$PWD/verdict-sdk/python/examples/openai-demo/traces/openai.jsonl"
CONFIG_FILE="$PWD/verdict-sdk/python/examples/openai-demo/verdict.yaml" # Use default config but ensure ID matches if we use generic

# We need the loop config again, or just reuse the one from smoke test if available.
# Let's recreate it to be safe and self-contained.
LOOP_CONFIG="$PWD/verdict-sdk/python/examples/openai-demo/verdict_loop_idemp.yaml"
cat > "$LOOP_CONFIG" <<EOF
version: 1
suite: openai-loop-idemp
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

# Truncate
: > "$TRACE_FILE"

# 1. First Run
echo "Run 1: Recording & CI"
export VERDICT_TRACE="$TRACE_FILE"
export OPENAI_API_KEY="mock"
export RECORDER_MODE="loop"

python3 verdict-sdk/python/examples/openai_record.py
$VERDICT ci --config "$LOOP_CONFIG" --trace-file "$TRACE_FILE" --db :memory: --replay-strict

# 2. Second Run (Re-run)
# Case A: Re-run entire workflow (truncate + record + ci).
# This proves process idempotency.
echo "Run 2: Re-Recording (Truncate) & CI"
: > "$TRACE_FILE"
python3 verdict-sdk/python/examples/openai_record.py

$VERDICT ci --config "$LOOP_CONFIG" --trace-file "$TRACE_FILE" --db :memory: --replay-strict

echo "✅ OpenAI Tool Loop Idempotency Test Passed"
