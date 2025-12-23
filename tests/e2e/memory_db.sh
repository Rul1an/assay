#!/bin/bash
set -e

# Setup
VERDICT=${VERDICT:-./target/debug/verdict}
EXAMPLE_DIR=examples/mcp-tool-safety-gate
TRACE_OUT=$EXAMPLE_DIR/traces/memory.trace.jsonl
rm -f $TRACE_OUT

echo "--- [MemoryDB] Step 1: Import ---"
$VERDICT trace import-mcp \
  --input $EXAMPLE_DIR/mcp/session.json \
  --format inspector \
  --episode-id demo \
  --test-id mcp_demo \
  --prompt "demo_user_prompt" \
  --out-trace $TRACE_OUT

echo "--- [MemoryDB] Step 2: Run with :memory: ---"
OUT=$($VERDICT ci \
  --config $EXAMPLE_DIR/verdict.yaml \
  --trace-file $TRACE_OUT \
  --db ":memory:" \
  --replay-strict 2>&1)

# Check 1: Success
if ! echo "$OUT" | grep -q "1 passed"; then
    echo "❌ Memory DB Run failed"
    exit 1
fi

# Check 2: Explicit Log
if ! echo "$OUT" | grep -q "auto-ingest: loaded"; then
    echo "❌ Missing auto-ingest log check"
    exit 1
fi

echo "✅ [MemoryDB] Success"
rm -f $TRACE_OUT
