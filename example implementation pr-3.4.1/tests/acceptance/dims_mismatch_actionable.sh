#!/bin/bash
# Acceptance test: Embedding dimensions mismatch shows actionable error
# DoD: Error shows both dimensions + model IDs + fix command

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TEST_DIR=$(mktemp -d)
trap "rm -rf $TEST_DIR" EXIT

echo "=== Test: Embedding dims mismatch shows actionable error ==="

# Create config with semantic_similarity metric
cat > "$TEST_DIR/eval.yaml" << 'EOF'
version: 1
suite: embedding-test
model: dummy
settings:
  embedding_model: text-embedding-3-small
tests:
  - id: t1
    input:
      prompt: "What is AI?"
    expected:
      type: semantic_similarity_to
      reference: "Artificial intelligence is a field of computer science."
      min_score: 0.8
EOF

# Create trace with DIFFERENT embedding model (different dimensions)
cat > "$TEST_DIR/traces.jsonl" << 'EOF'
{"prompt": "What is AI?", "response": "AI stands for artificial intelligence.", "meta": {"embeddings": {"model": "text-embedding-3-large", "dimensions": 3072, "vector": [0.1, 0.2]}}}
EOF

# Run with --replay-strict which should fail due to model mismatch
OUTPUT=$(verdict ci \
  --config "$TEST_DIR/eval.yaml" \
  --trace-file "$TEST_DIR/traces.jsonl" \
  --replay-strict 2>&1) || true

echo "$OUTPUT"

# Assertions
echo ""
echo "=== Assertions ==="

# Should contain error code E040 or E041 (dims or model mismatch)
if echo "$OUTPUT" | grep -qE "E04[01]"; then
  echo "✓ Contains embedding error code (E040/E041)"
else
  echo "✗ Missing embedding error code"
  exit 1
fi

# Should show expected dimensions (1536 for text-embedding-3-small)
if echo "$OUTPUT" | grep -q "1536"; then
  echo "✓ Contains expected dimensions (1536)"
else
  echo "✗ Missing expected dimensions"
  exit 1
fi

# Should show found dimensions (3072 for text-embedding-3-large)
if echo "$OUTPUT" | grep -q "3072"; then
  echo "✓ Contains found dimensions (3072)"
else
  echo "✗ Missing found dimensions"
  exit 1
fi

# Should show expected model name
if echo "$OUTPUT" | grep -q "text-embedding-3-small"; then
  echo "✓ Contains expected model name"
else
  echo "✗ Missing expected model name"
  exit 1
fi

# Should show found model name
if echo "$OUTPUT" | grep -q "text-embedding-3-large"; then
  echo "✓ Contains found model name"
else
  echo "✗ Missing found model name"
  exit 1
fi

# Should contain fix steps
if echo "$OUTPUT" | grep -q "Fix:"; then
  echo "✓ Contains fix steps"
else
  echo "✗ Missing fix steps"
  exit 1
fi

# Should suggest precompute command
if echo "$OUTPUT" | grep -q "precompute-embeddings"; then
  echo "✓ Suggests 'precompute-embeddings'"
else
  echo "✗ Missing precompute suggestion"
  exit 1
fi

echo ""
echo "=== PASS: Embedding dims mismatch test ==="
