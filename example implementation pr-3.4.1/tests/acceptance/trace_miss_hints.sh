#!/bin/bash
# Acceptance test: Trace miss should show closest match hints
# DoD: Error includes closest match with similarity score and diff highlighting

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR=$(mktemp -d)
trap "rm -rf $TEST_DIR" EXIT

echo "=== Test: Trace miss shows closest match hints ==="

# Create config with a test
cat > "$TEST_DIR/eval.yaml" << 'EOF'
version: 1
suite: trace-miss-test
model: dummy
tests:
  - id: t1
    input:
      prompt: "What is the capital of France?"
    expected:
      type: must_contain
      must_contain: ["Paris"]
EOF

# Create trace with slightly different prompt (capitol vs capital)
cat > "$TEST_DIR/traces.jsonl" << 'EOF'
{"prompt": "What is the capitol of France?", "response": "Paris is the capital of France."}
{"prompt": "What is the capital of Germany?", "response": "Berlin."}
EOF

# Run trace verify - should fail but show closest match
OUTPUT=$(verdict trace verify \
  --config "$TEST_DIR/eval.yaml" \
  --trace-file "$TEST_DIR/traces.jsonl" 2>&1) || true

echo "$OUTPUT"

# Assertions
echo ""
echo "=== Assertions ==="

# Should contain error code E001
if echo "$OUTPUT" | grep -q "E001"; then
  echo "✓ Contains error code E001"
else
  echo "✗ Missing error code E001"
  exit 1
fi

# Should mention the test ID
if echo "$OUTPUT" | grep -q "t1"; then
  echo "✓ Contains test ID 't1'"
else
  echo "✗ Missing test ID"
  exit 1
fi

# Should show the expected prompt
if echo "$OUTPUT" | grep -q "capital of France"; then
  echo "✓ Contains expected prompt"
else
  echo "✗ Missing expected prompt"
  exit 1
fi

# Should show closest match
if echo "$OUTPUT" | grep -q "capitol of France"; then
  echo "✓ Contains closest match (capitol)"
else
  echo "✗ Missing closest match"
  exit 1
fi

# Should show similarity score (should be > 0.9)
if echo "$OUTPUT" | grep -qE "similarity.*0\.9[0-9]"; then
  echo "✓ Contains similarity score > 0.9"
else
  echo "✗ Missing or incorrect similarity score"
  exit 1
fi

# Should contain fix steps
if echo "$OUTPUT" | grep -q "Fix:"; then
  echo "✓ Contains fix steps"
else
  echo "✗ Missing fix steps"
  exit 1
fi

# Should suggest trace ingest
if echo "$OUTPUT" | grep -q "verdict trace ingest"; then
  echo "✓ Suggests 'verdict trace ingest'"
else
  echo "✗ Missing ingest suggestion"
  exit 1
fi

echo ""
echo "=== PASS: Trace miss hints test ==="
