#!/bin/bash
# Integration test for environment variable scrubbing
# Run on Linux VM with assay installed

set -e

echo "=== Env Scrub Integration Tests ==="

# Setup test environment
export OPENAI_API_KEY="sk-test-secret-key"
export AWS_SECRET_ACCESS_KEY="aws-test-secret"
export GITHUB_TOKEN="ghp_test_token"
export MY_APP_TOKEN="app-token-123"
export NORMAL_VAR="this-should-pass"
export PATH="$PATH"
export HOME="$HOME"

# Test 1: Default scrub mode
echo ""
echo "Test 1: Default scrub (secrets should be removed)"
# Using 'printenv' in sandbox to see what environment variables exist
OUTPUT=$(assay sandbox -- printenv 2>/dev/null || true)

if echo "$OUTPUT" | grep -q "OPENAI_API_KEY"; then
    echo "FAIL: OPENAI_API_KEY should be scrubbed"
    exit 1
fi
if echo "$OUTPUT" | grep -q "AWS_SECRET_ACCESS_KEY"; then
    echo "FAIL: AWS_SECRET_ACCESS_KEY should be scrubbed"
    exit 1
fi
if echo "$OUTPUT" | grep -q "GITHUB_TOKEN"; then
    echo "FAIL: GITHUB_TOKEN should be scrubbed"
    exit 1
fi
if echo "$OUTPUT" | grep -q "MY_APP_TOKEN"; then
    echo "FAIL: MY_APP_TOKEN should be scrubbed"
    exit 1
fi
if ! echo "$OUTPUT" | grep -q "NORMAL_VAR"; then
    echo "FAIL: NORMAL_VAR should pass through"
    exit 1
fi
if ! echo "$OUTPUT" | grep -q "PATH"; then
    echo "FAIL: PATH should pass through"
    exit 1
fi
echo "PASS: Default scrub works correctly"

# Test 2: Explicit allow
echo ""
echo "Test 2: Explicit allow (--env-allow)"
OUTPUT=$(assay sandbox --env-allow OPENAI_API_KEY -- printenv 2>/dev/null || true)

if ! echo "$OUTPUT" | grep -q "OPENAI_API_KEY"; then
    echo "FAIL: OPENAI_API_KEY should be allowed through"
    exit 1
fi
if echo "$OUTPUT" | grep -q "AWS_SECRET_ACCESS_KEY"; then
    echo "FAIL: AWS_SECRET_ACCESS_KEY should still be scrubbed"
    exit 1
fi
echo "PASS: Explicit allow works correctly"

# Test 3: Multiple allows (comma-separated)
echo ""
echo "Test 3: Multiple allows (comma-separated)"
OUTPUT=$(assay sandbox --env-allow OPENAI_API_KEY,AWS_SECRET_ACCESS_KEY -- printenv 2>/dev/null || true)

if ! echo "$OUTPUT" | grep -q "OPENAI_API_KEY"; then
    echo "FAIL: OPENAI_API_KEY should be allowed"
    exit 1
fi
if ! echo "$OUTPUT" | grep -q "AWS_SECRET_ACCESS_KEY"; then
    echo "FAIL: AWS_SECRET_ACCESS_KEY should be allowed"
    exit 1
fi
if echo "$OUTPUT" | grep -q "GITHUB_TOKEN"; then
    echo "FAIL: GITHUB_TOKEN should still be scrubbed"
    exit 1
fi
echo "PASS: Multiple allows work correctly"

# Test 4: Passthrough mode (danger)
echo ""
echo "Test 4: Passthrough mode (--env-passthrough)"
OUTPUT=$(assay sandbox --env-passthrough -- printenv 2>/dev/null || true)

if ! echo "$OUTPUT" | grep -q "OPENAI_API_KEY"; then
    echo "FAIL: Passthrough should allow OPENAI_API_KEY"
    exit 1
fi
if ! echo "$OUTPUT" | grep -q "AWS_SECRET_ACCESS_KEY"; then
    echo "FAIL: Passthrough should allow AWS_SECRET_ACCESS_KEY"
    exit 1
fi
if ! echo "$OUTPUT" | grep -q "GITHUB_TOKEN"; then
    echo "FAIL: Passthrough should allow GITHUB_TOKEN"
    exit 1
fi
echo "PASS: Passthrough mode allows all vars"

# Test 5: Banner output check
echo ""
echo "Test 5: Banner output verification"
BANNER=$(assay sandbox -- true 2>&1 | head -20)

if ! echo "$BANNER" | grep -q "Env:"; then
    echo "FAIL: Banner should show Env: line"
    exit 1
fi
if ! echo "$BANNER" | grep -q "scrubbed\|clean"; then
    echo "FAIL: Banner should show scrubbed or clean status"
    exit 1
fi
echo "PASS: Banner output correct"

echo ""
echo "=== All Env Scrub Tests Passed ==="
