#!/bin/bash
# Acceptance test: Baseline mismatch shows actionable error
# DoD: Error shows suite/schema differences + regenerate command

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR=$(mktemp -d)
trap "rm -rf $TEST_DIR" EXIT

echo "=== Test: Baseline mismatch shows actionable error ==="

# Create config with suite name "prod-tests"
cat > "$TEST_DIR/eval.yaml" << 'EOF'
version: 1
suite: prod-tests
model: dummy
tests:
  - id: t1
    input:
      prompt: "Hello"
    expected:
      type: must_contain
      must_contain: ["Hello"]
EOF

# Create baseline with DIFFERENT suite name "dev-tests"
cat > "$TEST_DIR/baseline.json" << 'EOF'
{
  "schema_version": "v1",
  "verdict_version": "0.3.3",
  "suite": "dev-tests",
  "created_at": "2025-12-21T10:00:00Z",
  "tests": {
    "t1": {
      "scores": {
        "must_contain": 1.0
      }
    }
  }
}
EOF

# Run with baseline - should fail due to suite mismatch
OUTPUT=$(verdict ci \
  --config "$TEST_DIR/eval.yaml" \
  --baseline "$TEST_DIR/baseline.json" 2>&1) || true

echo "$OUTPUT"

# Assertions
echo ""
echo "=== Assertions ==="

# Should contain error code E021 (suite mismatch)
if echo "$OUTPUT" | grep -q "E021"; then
  echo "✓ Contains error code E021 (suite mismatch)"
else
  echo "✗ Missing error code E021"
  exit 1
fi

# Should show expected suite name
if echo "$OUTPUT" | grep -q "prod-tests"; then
  echo "✓ Contains expected suite name (prod-tests)"
else
  echo "✗ Missing expected suite name"
  exit 1
fi

# Should show found suite name
if echo "$OUTPUT" | grep -q "dev-tests"; then
  echo "✓ Contains found suite name (dev-tests)"
else
  echo "✗ Missing found suite name"
  exit 1
fi

# Should contain fix steps
if echo "$OUTPUT" | grep -q "Fix:"; then
  echo "✓ Contains fix steps"
else
  echo "✗ Missing fix steps"
  exit 1
fi

# Should suggest regenerating baseline or updating config
if echo "$OUTPUT" | grep -qE "(baseline save|regenerate|Update config)"; then
  echo "✓ Suggests baseline regeneration or config update"
else
  echo "✗ Missing actionable suggestion"
  exit 1
fi

echo ""
echo "=== PASS: Baseline mismatch test ==="
