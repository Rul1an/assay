#!/bin/bash
set -e

# Setup
VERDICT=${VERDICT:-./target/debug/verdict}
EXAMPLE_DIR=examples/mcp-tool-safety-gate
TRACE_OUT=$EXAMPLE_DIR/traces/idempotency.trace.jsonl
DB_OUT=$EXAMPLE_DIR/.eval/idempotency.db
rm -f $TRACE_OUT $DB_OUT

echo "--- [Idempotency] Step 1: Import ---"
$VERDICT trace import-mcp \
  --input $EXAMPLE_DIR/mcp/session.json \
  --format inspector \
  --episode-id demo \
  --test-id mcp_demo \
  --prompt "demo_user_prompt" \
  --out-trace $TRACE_OUT

echo "--- [Idempotency] Step 2: Run 1 (Auto-Ingest) ---"
OUT1=$($VERDICT ci \
  --config $EXAMPLE_DIR/verdict.yaml \
  --trace-file $TRACE_OUT \
  --db $DB_OUT \
  --replay-strict 2>&1)

if ! echo "$OUT1" | grep -q "1 passed"; then
    echo "❌ Run 1 failed"
    exit 1
fi

echo "--- [Idempotency] Step 3: Run 2 (Re-Ingest) ---"
OUT2=$($VERDICT ci \
  --config $EXAMPLE_DIR/verdict.yaml \
  --trace-file $TRACE_OUT \
  --db $DB_OUT \
  --replay-strict 2>&1)

if ! echo "$OUT2" | grep -q "1 passed"; then
    echo "❌ Idempotency Check Failed (Run 2 crashed)"
    echo "$OUT2"
    exit 1
fi

echo "✅ [Idempotency] Success"
rm -f $TRACE_OUT $DB_OUT
