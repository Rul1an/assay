#!/bin/bash
set -e

APP="${APP:-target/debug/assay}"
TEST_DIR="tests/tmp/import_smoke"
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR"

if [ ! -f "$APP" ]; then
    echo "Rebuilding binary..."
    cargo build -q
    APP="target/debug/assay"
fi

echo "🧪 [Importer] Starting smoke test..."

# 1. Create dummy MCP JSON-RPC transcript
cat > "$TEST_DIR/transcript.jsonl" <<EOF
{"jsonrpc":"2.0","method":"tools/call","params":{"name":"tool_A","arguments":{}}}
{"jsonrpc":"2.0","method":"tools/call","params":{"name":"tool_B","arguments":{}}}
EOF

# 2. Run import
cd "$TEST_DIR"
echo "👉 Running import..."
"../../../$APP" import --init --format jsonrpc transcript.jsonl

# 3. Verify mcp-eval.yaml content
if [ ! -f "mcp-eval.yaml" ]; then
    echo "❌ mcp-eval.yaml NOT created"
    exit 1
fi

CONTENT=$(cat mcp-eval.yaml)

# Check Config Version
if echo "$CONTENT" | grep -q "configVersion: 1"; then
    echo "✅ configVersion: 1 found"
else
    echo "❌ configVersion: 1 MISSING"
    echo "$CONTENT"
    exit 1
fi

# Check Rules (DSL) instead of sequence
if echo "$CONTENT" | grep -q "rules:"; then
    echo "✅ rules: DSL found"
else
    echo "❌ rules: DSL MISSING"
    echo "$CONTENT"
    exit 1
fi

# Check Require rules
if echo "$CONTENT" | grep -q "type: require" && echo "$CONTENT" | grep -q "tool: \"tool_A\""; then
    echo "✅ Require rule for tool_A found"
else
    echo "❌ Require rule for tool_A MISSING"
    echo "$CONTENT"
    exit 1
fi

echo "🎉 All Importer tests passed!"
