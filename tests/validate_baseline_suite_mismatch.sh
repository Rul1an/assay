#!/bin/bash
set -e

VERDICT_BIN="cargo run -q --bin verdict --"
TEST_DIR=$(mktemp -d)
trap 'rm -rf "$TEST_DIR"' EXIT

cat <<EOF > "$TEST_DIR/eval.yaml"
version: 1
suite: demo_suite_A
model: dummy
tests:
  - id: t1
    input: { prompt: "foo" }
    expected: { type: must_contain, must_contain: ["bar"] }
EOF

# Baseline for suite B
cat <<EOF > "$TEST_DIR/baseline.json"
{
  "schema_version": 1,
  "suite": "demo_suite_B",
  "verdict_version": "0.1.0",
  "created_at": "2024-01-01T00:00:00Z",
  "config_fingerprint": "abc",
  "entries": []
}
EOF

echo "Running verdict validate (expecting E_BASE_MISMATCH)..."
set +e
cargo run -q --bin verdict -- validate --config "$TEST_DIR/eval.yaml" --baseline "$TEST_DIR/baseline.json" > "$TEST_DIR/stdout.log" 2> "$TEST_DIR/stderr.log"
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

if ! grep -q "E_BASE_MISMATCH" "$TEST_DIR/stderr.log"; then
  echo "FAIL: Output does not contain E_BASE_MISMATCH"
  cat "$TEST_DIR/stderr.log"
  exit 1
fi

echo "PASS: E_BASE_MISMATCH detected correctly"
