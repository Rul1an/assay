#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EXAMPLE="$ROOT/examples/ci-regression-gate"
BIN="$ROOT/target/debug/verdict"

# Ensure binary is built
cargo build -q --bin verdict

cd "$EXAMPLE"
rm -rf .eval

test -f baseline.json || (echo "Missing baseline.json. Run export_baseline_local.sh first." && exit 2)

echo "Running Gate against pr_bad.jsonl (EXPECTING FAILURE)..."

set +e # Allow failure for demo purposes
"$BIN" ci \
  --config eval.yaml \
  --trace-file traces/pr_bad.jsonl \
  --replay-strict \
  --baseline baseline.json \
  --db .eval/eval.db
EXIT_CODE=$?
set -e

if [ $EXIT_CODE -ne 0 ]; then
  echo "✅ Gate CORRECTLY failed (Exit Code: $EXIT_CODE)"
  echo "   (This is expected behavior for regression demo)"
else
  echo "❌ Gate UNEXPECTEDLY passed!"
  exit 1
fi
