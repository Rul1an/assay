#!/bin/bash
set -e


TEST_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_DIR"' EXIT

# Config requiring semantic similarity (implies embeddings)
cat <<EOF > "$TEST_DIR/eval.yaml"
version: 1
suite: demo
model: dummy
tests:
  - id: t1
    input:
      prompt: "hello"
    expected:
      type: semantic_similarity_to
      text: "hi"
      threshold: 0.8
EOF

# Trace file matching prompt but MISSING embeddings
cat <<EOF > "$TEST_DIR/trace.jsonl"
{"schema_version":1,"type":"verdict.trace","prompt":"hello","response":"hi there","model":"gpt-4","meta":{"verdict":{}}}
EOF

echo "Running verdict validate (expecting E_REPLAY_STRICT_MISSING)..."
set +e
cargo run -q --bin verdict -- validate --config "$TEST_DIR/eval.yaml" --trace-file "$TEST_DIR/trace.jsonl" --replay-strict > "$TEST_DIR/stdout.log" 2> "$TEST_DIR/stderr.log"
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

if ! grep -q "E_REPLAY_STRICT_MISSING" "$TEST_DIR/stderr.log"; then
  echo "FAIL: Output does not contain E_REPLAY_STRICT_MISSING"
  cat "$TEST_DIR/stderr.log"
  exit 1
fi

echo "PASS: E_REPLAY_STRICT_MISSING detected correctly"
