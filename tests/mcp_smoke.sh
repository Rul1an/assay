#!/bin/bash
set -e

# Setup
TEST_DIR="tests/tmp/mcp_smoke"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"
FIXTURE="tests/fixtures/mcp/edge_case.json"

# Build (optional, ensure debug build is fresh)
cargo build --quiet

APP="target/debug/assay"

echo "Running MCP Smoke Test in $TEST_DIR..."

cd "$TEST_DIR"

# 1. Import
echo "1. Importing fixture..."
../../../$APP import --format mcp-inspector ../../../$FIXTURE --init --out-trace trace.jsonl

# 2. Check Scaffolding
if [ ! -f "mcp-eval.yaml" ]; then
    echo "❌ mcp-eval.yaml missing"
    exit 1
fi
if [ -d "policies" ]; then
    echo "❌ policies/ directory should NOT exist (switched to inline config)"
    exit 1
fi

# 3. Verify Schema Content (Inline Guardrail)
echo "2. Verifying Inline Guardrails..."
if ! grep -q "properties: {}" mcp-eval.yaml; then
    echo "❌ mcp-eval.yaml does not contain 'properties: {}' (inline schema check)"
    cat mcp-eval.yaml
    exit 1
fi
if ! grep -q '\- "valid_tool"' mcp-eval.yaml; then
    echo "❌ mcp-eval.yaml does not contain valid_tool in sequence (inline sequence check)"
    cat mcp-eval.yaml
    exit 1
fi

# 4. Run Verification (Strict)
echo "3. Running verification (strict replay)..."
../../../$APP run --config mcp-eval.yaml --trace-file trace.jsonl --replay-strict --no-cache

echo "✅ MCP Smoke Test Passed!"
cd -
rm -rf "$TEST_DIR"
