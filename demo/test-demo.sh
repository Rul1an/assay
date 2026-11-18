#!/bin/bash
set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

echo "🔍 Running Demo Contract Test..."

# 1. Preflight: Check keys
grep -q "ignore" demo/fixtures/traces/safe.jsonl || { echo -e "${RED}FAIL: safe.jsonl missing 'ignore' key${NC}"; exit 1; }
grep -q "ignore" demo/fixtures/traces/unsafe.jsonl || { echo -e "${RED}FAIL: unsafe.jsonl missing 'ignore' key${NC}"; exit 1; }

# The fixture's eval.yaml contains the policy used for both replay traces.
BINARY_PATH="$(pwd)/target/debug/assay"
cd demo/fixtures

# 2. Run Safe Trace (Expected: 0)
echo "-----------------------------------"
echo "Testing Safe Trace (Should PASS)..."
$BINARY_PATH run --config eval.yaml --trace-file traces/safe.jsonl
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✅ Safe Trace PASSED${NC}"
else
    echo -e "${RED}❌ Safe Trace FAILED${NC}"
    exit 1
fi

# 3. Run Unsafe Trace (Expected: 1, E_POLICY_VIOLATION)
echo "-----------------------------------"
echo "Testing Unsafe Trace (Should FAIL)..."
set +e
$BINARY_PATH run --config eval.yaml --trace-file traces/unsafe.jsonl > /dev/null 2>&1
EXIT_CODE=$?
set -e

if [ $EXIT_CODE -eq 1 ]; then
    echo -e "${GREEN}✅ Unsafe Trace FAILED as expected (Exit 1)${NC}"
elif [ $EXIT_CODE -eq 2 ]; then
    echo -e "${RED}❌ Unsafe Trace FAILED with CONFIG ERROR (Exit 2) - Likely E_TRACE_MISS${NC}"
    exit 1
else
    echo -e "${RED}❌ Unsafe Trace exited with code $EXIT_CODE (Expected 1)${NC}"
    exit 1
fi

echo "-----------------------------------"
echo -e "${GREEN}🎉 Demo Contract Verified! Fixtures are robust.${NC}"
