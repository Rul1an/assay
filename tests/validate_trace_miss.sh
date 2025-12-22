#!/bin/bash
set -e

# Path to the compiled binary
VERDICT_BIN="cargo run -q --bin verdict --"

# Create a temporary directory for the test
TEST_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_DIR"' EXIT

# Create a configuration file
cat <<EOF > "$TEST_DIR/eval.yaml"
version: 1
suite: demo
model: dummy
tests:
  - id: t1
    input:
      prompt: "missing_prompt"
    expected:
      type: must_contain
      must_contain: ["world"]
EOF

# Create a trace file without the prompt
cat <<EOF > "$TEST_DIR/trace.jsonl"
{"schema_version":1,"type":"verdict.trace","prompt":"other_prompt","response":"foo","model":"gpt-4"}
EOF

# Run verdict validate
echo "Running verdict validate (expecting E_TRACE_MISS)..."
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
  echo "EVAL CONTENT:"
  cat "$TEST_DIR/eval.yaml"
  echo "TRACE CONTENT:"
  cat "$TEST_DIR/trace.jsonl"
  exit 1
fi

if [ "$EXIT_CODE" -ne 2 ]; then
  echo "FAIL: Expected exit code 2, got $EXIT_CODE"
  exit 1
fi

if ! grep -q "E_TRACE_MISS" "$TEST_DIR/stderr.log"; then
  echo "FAIL: Output does not contain E_TRACE_MISS"
  cat "$TEST_DIR/stderr.log"
  exit 1
fi

echo "PASS: E_TRACE_MISS detected correctly"
