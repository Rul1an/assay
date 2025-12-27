#!/bin/bash
set -e

APP="target/debug/assay"
FIXTURE_DIR="tests/fixtures/migration"
TEMP_YAML="tests/tmp/equivalence/migrated.yaml"

mkdir -p "tests/tmp/equivalence"

# Copy legacy to temp
cp "$FIXTURE_DIR/complex_legacy.yaml" "$TEMP_YAML"

echo "1. Running migration logic..."
$APP migrate --config "$TEMP_YAML"

echo "2. Comparing with expected output..."
# Use diff. Normalized whitespace? YAML cares about whitespace.
# But we should be exact if possible.
if diff -u "$FIXTURE_DIR/complex_expected.yaml" "$TEMP_YAML"; then
    echo "✅ Files match perfectly."
else
    echo "❌ Files do not match!"
    exit 1
fi

echo "✅ Equivalence Test Passed!"
