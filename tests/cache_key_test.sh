#!/bin/bash
set -e

APP="target/debug/assay"
TEST_DIR="tests/tmp/cache_key"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR/policies"

# 1. Setup
cat > "$TEST_DIR/policies/args.yaml" <<EOF
type: object
properties:
  foo: { type: string }
EOF

cat > "$TEST_DIR/eval.yaml" <<EOF
version: 1
suite: cache-test
model: fake-gpt
tests:
  - id: cache-test-1
    input: { prompt: "test" }
    expected:
      type: args_valid
      policy: policies/args.yaml
EOF

cd "$TEST_DIR"

echo "1. First Run (Miss)..."
OUTPUT1=$(../../../$APP run --config eval.yaml 2>&1 || true)
if [[ "$OUTPUT1" == *"[CACHE HIT]"* ]]; then
    echo "❌ Unexpected cache hit on first run"
    exit 1
fi

echo "2. Second Run (Hit)..."
OUTPUT2=$(../../../$APP run --config eval.yaml 2>&1 || true)
if [[ "$OUTPUT2" != *"[CACHE HIT]"* ]]; then
   # Note: fake-gpt doesn't really "cache" unless we enable cache.
   # Settings cache default is true.
   # runner.rs checks self.cache.get(&key).
   # VcrCache is filesystem based.
   echo "❌ Expected cache hit on second run"
   echo "Output: $OUTPUT2"
   exit 1
fi

echo "3. Modify Policy..."
cat > "policies/args.yaml" <<EOF
type: object
properties:
  bar: { type: integer }
EOF

echo "4. Third Run (Miss due to policy change)..."
OUTPUT3=$(../../../$APP run --config eval.yaml 2>&1 || true)
if [[ "$OUTPUT3" == *"[CACHE HIT]"* ]]; then
    echo "❌ Unexpected cache hit after policy change!"
    exit 1
fi

echo "✅ Cache Key Hardening Verified!"
