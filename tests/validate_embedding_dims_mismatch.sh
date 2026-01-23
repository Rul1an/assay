#!/bin/bash
set -e


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

# Trace with empty embedding vector (invalid)
cat <<EOF > "$TEST_DIR/trace.jsonl"
{"schema_version":1,"type":"verdict.trace","prompt":"hello","response":"hi","model":"gpt-4","meta":{"verdict":{"embeddings":{"response":[]}}}}
EOF

echo "Running verdict validate (expecting E_EMB_DIMS)..."
set +e
cargo run -q --bin verdict -- validate --config "$TEST_DIR/eval.yaml" --trace-file "$TEST_DIR/trace.jsonl" > "$TEST_DIR/stdout.log" 2> "$TEST_DIR/stderr.log"
EXIT_CODE=$?
set -e
echo "Actual Exit Code: $EXIT_CODE"

if [ "$EXIT_CODE" -eq 0 ]; then
  echo "FAIL: Expected command to fail with exit code 2, passed with 0"
  echo "STDOUT IS:"
  cat "$TEST_DIR/stdout.log"
  echo "STDERR IS:"
  cat "$TEST_DIR/stderr.log"
  exit 1
fi

if [ "$EXIT_CODE" -ne 2 ]; then
  echo "FAIL: Expected exit code 2, got $EXIT_CODE"
  exit 1
fi

if ! grep -q "E_EMB_DIMS" "$TEST_DIR/stderr.log"; then
  echo "FAIL: Output does not contain E_EMB_DIMS"
  cat "$TEST_DIR/stderr.log"
  exit 1
fi

echo "PASS: E_EMB_DIMS detected correctly"
