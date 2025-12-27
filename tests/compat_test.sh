#!/bin/bash
set -e

APP="target/debug/assay"
FIXTURE_DIR="tests/fixtures/migration"

echo "1. Testing Anchors..."
$APP migrate --config "$FIXTURE_DIR/compat_anchors.yaml" --dry-run > /dev/null
# Verify anchor expansion happened if we output valid YAML
# But dry-run output is minimal.
# Let's run it.
OUTPUT=$($APP run --config "$FIXTURE_DIR/compat_anchors.yaml" 2>&1 || true)
if [[ "$OUTPUT" == *"Error"* ]]; then
    echo "❌ Anchors failed: $OUTPUT"
    exit 1
fi

echo "2. Testing Unknown Fields..."
OUTPUT=$($APP run --config "$FIXTURE_DIR/compat_unknown.yaml" 2>&1 || true)
if [[ "$OUTPUT" == *"Error"* && "$OUTPUT" != *"[E_TRACE_MISS]"* ]]; then
   # Note: E_TRACE_MISS is expected because we don't have traces.
   # But if it errors on PARSING, that's bad.
   # "unknown field" error usually comes from serde during deserialization.
   if [[ "$OUTPUT" == *"unknown field"* ]]; then
       echo "❌ Unknown fields failed validation: $OUTPUT"
       exit 1
   fi
fi

# Check if it parsed successfully by checking suite name in start log?
# Or checking if it managed to run 1 test.
if [[ "$OUTPUT" == *"Running 1 tests"* ]]; then
    echo "✅ Unknown fields ignored successfully."
else
    # E_TRACE_MISS means it parsed and tried to run.
    if [[ "$OUTPUT" == *"Trace miss"* ]]; then
        echo "✅ Unknown fields ignored successfully (failed later on trace)."
    else
        echo "❌ Unexpected output: $OUTPUT"
        exit 1
    fi
fi

echo "✅ Compatibility Tests Passed!"
