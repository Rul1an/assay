#!/bin/bash
set -e
cd "$(dirname "$0")"

# Ensure .eval directory exists
mkdir -p .eval

# Locate binary (assuming we are in examples/agent-function-calling/)
# ../../target/debug/verdict relative to this script
VERDICT_BIN="../../target/debug/verdict"

if [ ! -f "$VERDICT_BIN" ]; then
    echo "Error: Binary not found at $VERDICT_BIN (PWD: $(pwd))"
    exit 1
fi

echo "Using Verdict binary: $VERDICT_BIN"

# 1. Ingest OTel Trace (linked to suite) & emit Replay File
echo "Ingesting OTel trace..."
$VERDICT_BIN trace ingest-otel \
    --input otel_trace.jsonl \
    --db .eval/eval.db \
    --suite web-search-suite \
    --out-trace otel.v2.jsonl

# 2. Run Gate (using V2 Trace File for replay)
echo "Running Gate..."
$VERDICT_BIN ci \
    --config eval.yaml \
    --db .eval/eval.db \
    --trace-file otel.v2.jsonl \
    --replay-strict
