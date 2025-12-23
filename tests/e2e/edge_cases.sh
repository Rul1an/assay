#!/bin/bash
set -e

# Setup
VERDICT=${VERDICT:-./target/debug/verdict}
EXAMPLE_DIR=examples/mcp-tool-safety-gate
TRACE_OUT=$EXAMPLE_DIR/traces/edge.trace.jsonl
DB_OUT=":memory:"
rm -f $TRACE_OUT verdict_mismatch.yaml verdict_multi.yaml verdict_prompt_fail.yaml trace_A.jsonl trace_B.jsonl combined.jsonl

echo "--- [Edge] Setup: Import ---"
$VERDICT trace import-mcp \
  --input $EXAMPLE_DIR/mcp/session.json \
  --format inspector \
  --episode-id mcp_demo \
  --test-id mcp_demo \
  --prompt "demo_user_prompt" \
  --out-trace $TRACE_OUT

echo "--- [Edge] Test 1: ID Mismatch (Robustness) ---"
cat > verdict_mismatch.yaml <<EOF
version: 1
suite: "mcp_mismatch_suite"
model: "trace"
tests:
  - id: mcp_ghost
    input:
      prompt: "demo_user_prompt"
    expected:
      type: regex_match
      pattern: ".*"
    assert:
      - type: trace_must_call_tool
        tool: "ApplyDiscount"
EOF

OUT_MISMATCH=$($VERDICT ci \
  --config verdict_mismatch.yaml \
  --trace-file $TRACE_OUT \
  --db $DB_OUT \
  --replay-strict 2>&1)

if echo "$OUT_MISMATCH" | grep -q "1 passed"; then
    echo "✅ ID Mismatch passed (Robust Replay)"
else
    echo "❌ ID Mismatch failed"
    exit 1
fi

echo "--- [Edge] Test 2: Multi-Episode Disambiguation ---"
# Generate A & B
$VERDICT trace import-mcp \
  --input $EXAMPLE_DIR/mcp/session.json \
  --format inspector \
  --episode-id ep_A \
  --test-id test_A \
  --prompt "prompt_A" \
  --out-trace trace_A.jsonl

$VERDICT trace import-mcp \
  --input $EXAMPLE_DIR/mcp/session.json \
  --format inspector \
  --episode-id ep_B \
  --test-id test_B \
  --prompt "prompt_B" \
  --out-trace trace_B.jsonl

cat trace_A.jsonl trace_B.jsonl > combined.jsonl

cat > verdict_multi.yaml <<EOF
version: 1
suite: "mcp_multi_suite"
model: "trace"
tests:
  - id: test_B
    input:
      prompt: "prompt_B"
    expected:
      type: regex_match
      pattern: ".*"
    assert:
      - type: trace_must_call_tool
        tool: "ApplyDiscount"
EOF

OUT_MULTI=$($VERDICT ci \
  --config verdict_multi.yaml \
  --trace-file combined.jsonl \
  --db $DB_OUT \
  --replay-strict 2>&1)

if echo "$OUT_MULTI" | grep -q "1 passed"; then
    echo "✅ Multi-Episode Disambiguation passed"
else
    echo "❌ Multi-Episode failed"
    exit 1
fi

echo "--- [Edge] Test 3: Prompt Mismatch (Strictness) ---"
cat > verdict_prompt_fail.yaml <<EOF
version: 1
suite: "mcp_prompt_fail"
model: "trace"
tests:
  - id: mcp_demo
    input:
      prompt: "UNKNOWN_PROMPT"
    expected:
      type: regex_match
      pattern: ".*"
    assert:
      - type: trace_must_call_tool
        tool: "ApplyDiscount"
EOF

set +e
OUT_PROMPT=$($VERDICT ci \
  --config verdict_prompt_fail.yaml \
  --trace-file $TRACE_OUT \
  --db $DB_OUT \
  --replay-strict 2>&1)
EXIT_P=$?
set -e

if [ $EXIT_P -ne 0 ]; then
    echo "✅ Prompt Mismatch failed as expected"
else
    echo "❌ Prompt Mismatch passed unexpectedly"
    exit 1
fi

# Cleanup
rm -f $TRACE_OUT verdict_mismatch.yaml verdict_multi.yaml verdict_prompt_fail.yaml trace_A.jsonl trace_B.jsonl combined.jsonl
echo "✅ [Edge] All Passed"
