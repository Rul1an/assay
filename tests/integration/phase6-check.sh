#!/bin/bash
set -e
# Phase 6 Refined Integration Check
# SOTA Patterns, Safe Path, Fail-Closed Marker

BINARY="./target/debug/assay"
if [ ! -f "$BINARY" ]; then
    echo "Binary not found, building..."
    cargo build -p assay-cli
fi

echo "1. Checking --env-strict (Only safe vars pass)"
OUTPUT=$(AWS_SECRET_ACCESS_KEY="secret" PATH="/bin:/usr/bin" $BINARY sandbox --env-strict -- env)

if echo "$OUTPUT" | grep -q "AWS_SECRET"; then
  echo "FAIL: AWS_SECRET leaked in strict mode"
  exit 1
fi
if ! echo "$OUTPUT" | grep -q "PATH"; then
  echo "FAIL: PATH missing in strict mode"
  exit 1
fi
echo "PASS: Strict mode filters correctly"

echo "2. Checking --env-safe-path"
# Should overwrite PATH
# We use --env-passthrough false (default) or strict.
# --env-safe-path works in any mode probably? It post-processes.
OUTPUT=$(PATH="/custom/path" $BINARY sandbox --env-safe-path -- env)
if echo "$OUTPUT" | grep -q "/custom/path"; then
  echo "FAIL: PATH not reset by --env-safe-path"
  echo "Got output:"
  echo "$OUTPUT"
  exit 1
fi
# Check for safe default parts
if ! echo "$OUTPUT" | grep -q "/usr/bin"; then
  echo "FAIL: Safe path missing /usr/bin"
  exit 1
fi
echo "PASS: Safe Path works"

echo "3. Checking Exec-Influence Stripping (SOTA)"
# Test with ZDOTDIR (shell influence)
OUTPUT=$(ZDOTDIR="/tmp/evil" $BINARY sandbox --env-strip-exec -- env)
if echo "$OUTPUT" | grep -q "^ZDOTDIR="; then
  echo "FAIL: ZDOTDIR not stripped"
  exit 1
fi
echo "PASS: SOTA influence stripping works"

echo "4. Checking Fail-Closed"
if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "SKIP: Fail-closed is Linux-only (Landlock)"
else
  cat > conflict.yaml <<EOF
version: "1.0"
name: "conflict"
fs:
  allow:
     - path: "${HOME}/**"
       read: true
  deny:
     - path: "${HOME}/.ssh/**"
net:
  mode: audit
EOF

  set +e
  $BINARY sandbox --fail-closed --policy conflict.yaml -- true 2> err.txt
  CODE=$?
  set -e

  if [ $CODE -ne 2 ]; then
    echo "FAIL: Expected exit code 2, got $CODE"
    cat err.txt
    exit 1
  fi

  if ! grep -q "E_POLICY_CONFLICT_DENY_WINS_UNENFORCEABLE" err.txt; then
    echo "FAIL: Missing compatibility marker"
    cat err.txt
    exit 1
  fi
  echo "PASS: Fail-closed marker verified"
  rm conflict.yaml err.txt
fi

echo "Phase 6 Refined Checks Passed! 🛡️"
