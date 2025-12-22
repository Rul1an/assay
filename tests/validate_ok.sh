#!/bin/bash
set -e

VERDICT_BIN="cargo run -q --bin verdict --"
TEST_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_DIR"' EXIT

cat <<EOF > "$TEST_DIR/eval.yaml"
version: 1
suite: demo
model: dummy
tests:
  - id: t1
    input:
       prompt: "hello"
    expected:
       type: must_contain
       must_contain: ["hi"]
EOF

cat <<EOF > "$TEST_DIR/trace.jsonl"
{"schema_version":1,"type":"verdict.trace","prompt":"hello","response":"hi","model":"gpt-4","meta":{"verdict":{"embeddings":{"response":[0.1, 0.2]}}}}
EOF

echo "Running verdict validate (expecting OK)..."
if ! cargo run -q --bin verdict -- validate --config "$TEST_DIR/eval.yaml" --trace-file "$TEST_DIR/trace.jsonl" --format json > "$TEST_DIR/stdout.log" 2> "$TEST_DIR/stderr.log"; then
  echo "FAIL: Command failed"
  cat "$TEST_DIR/stderr.log"
  exit 1
fi

echo "Checking JSON output..."
cat "$TEST_DIR/stdout.log"

if ! grep -q '"ok": true' "$TEST_DIR/stdout.log"; then
  echo "FAIL: JSON output ok != true"
  exit 1
fi

echo "PASS: Validation succeeded"
