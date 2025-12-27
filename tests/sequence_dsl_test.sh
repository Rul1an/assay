#!/bin/bash
set -e

# Setup
APP="${APP:-target/debug/assay}"
TEST_DIR="tests/tmp/sequence_dsl_smoke"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"

if [ ! -f "$APP" ]; then
    echo "Rebuilding binary..."
    cargo build -q
    APP="target/debug/assay"
fi

echo "🧪 [Sequence DSL] Starting smoke test..."

# 1. Create a dummy trace with sequence: [init, work, shutdown]
cat > "$TEST_DIR/trace.jsonl" <<EOF
{"type":"episode_start","episode_id":"ep1","timestamp":0,"input":{"prompt":"test"},"meta":{}}
{"type":"step","episode_id":"ep1","step_id":"s_llm","idx":0,"timestamp":10,"kind":"llm_completion","name":"gpt","content":"thought","meta":{}}
{"type":"tool_call","episode_id":"ep1","step_id":"s1","timestamp":100,"tool_name":"init","args":{},"call_index":0}
{"type":"tool_call","episode_id":"ep1","step_id":"s2","timestamp":200,"tool_name":"work","args":{},"call_index":1}
{"type":"tool_call","episode_id":"ep1","step_id":"s3","timestamp":300,"tool_name":"shutdown","args":{},"call_index":2}
{"type":"episode_end","episode_id":"ep1","timestamp":400,"outcome":"pass"}
EOF

# 2. Create PASS config (valid rules)
# - Require: init (present)
# - Before: init < shutdown (true)
# - Blocklist: "format" (not present)
cat > "$TEST_DIR/pass.yaml" <<EOF
version: 1
suite: "dsl_pass"
model: "dummy"
tests:
  - id: "dsl_pass_1"
    input:
      prompt: "test"
    expected:
      type: sequence_valid
      rules:
        - type: require
          tool: "init"
        - type: before
          first: "init"
          then: "shutdown"
        - type: blocklist
          pattern: "format"
EOF

echo "👉 Running PASS scenario..."
ASSAY_LOG=debug "$APP" run --config "$TEST_DIR/pass.yaml" --trace-file "$TEST_DIR/trace.jsonl" --strict > "$TEST_DIR/pass.out" 2>&1
RESULT=$?
if [ $RESULT -eq 0 ]; then
    echo "✅ PASS scenario succeeded"
else
    echo "❌ PASS scenario FAILED"
    cat "$TEST_DIR/pass.out"
    exit 1
fi

# 3. Create FAIL config (missing requirement)
cat > "$TEST_DIR/fail_req.yaml" <<EOF
version: 1
suite: "dsl_fail_req"
model: "dummy"
tests:
  - id: "dsl_fail_1"
    input:
      prompt: "test"
    expected:
      type: sequence_valid
      rules:
        - type: require
          tool: "missing_tool"
EOF

echo "👉 Running FAIL (require) scenario..."
"$APP" run --config "$TEST_DIR/fail_req.yaml" --trace-file "$TEST_DIR/trace.jsonl" --strict > "$TEST_DIR/fail_req.out" 2>&1 || true
if grep -q "failed: sequence_valid" "$TEST_DIR/fail_req.out"; then
    echo "✅ FAIL (require) caught correctly"
else
    echo "❌ FAIL (require) did NOT catch missing tool"
    cat "$TEST_DIR/fail_req.out"
    exit 1
fi

# 4. Create FAIL config (bad order)
# Rule: shutdown before init (trace has init then shutdown)
cat > "$TEST_DIR/fail_order.yaml" <<EOF
version: 1
suite: "dsl_fail_order"
model: "dummy"
tests:
  - id: "dsl_fail_2"
    input:
      prompt: "test"
    expected:
      type: sequence_valid
      rules:
        - type: before
          first: "shutdown"
          then: "init"
EOF

echo "👉 Running FAIL (order) scenario..."
"$APP" run --config "$TEST_DIR/fail_order.yaml" --trace-file "$TEST_DIR/trace.jsonl" --strict > "$TEST_DIR/fail_order.out" 2>&1 || true
if grep -q "failed: sequence_valid" "$TEST_DIR/fail_order.out"; then
    echo "✅ FAIL (order) caught correctly"
else
    echo "❌ FAIL (order) did NOT catch order mismatch"
    cat "$TEST_DIR/fail_order.out"
    exit 1
fi

# 5. Create FAIL config (blocklist)
cat > "$TEST_DIR/fail_block.yaml" <<EOF
version: 1
suite: "dsl_fail_block"
model: "dummy"
tests:
  - id: "dsl_fail_3"
    input:
      prompt: "test"
    expected:
      type: sequence_valid
      rules:
        - type: blocklist
          pattern: "work"
EOF

echo "👉 Running FAIL (blocklist) scenario..."
"$APP" run --config "$TEST_DIR/fail_block.yaml" --trace-file "$TEST_DIR/trace.jsonl" --strict > "$TEST_DIR/fail_block.out" 2>&1 || true
if grep -q "failed: sequence_valid" "$TEST_DIR/fail_block.out"; then
    echo "✅ FAIL (blocklist) caught correctly"
else
    echo "❌ FAIL (blocklist) did NOT catch blocked tool"
    cat "$TEST_DIR/fail_block.out"
    exit 1
fi

echo "🎉 All Sequence DSL tests passed!"
