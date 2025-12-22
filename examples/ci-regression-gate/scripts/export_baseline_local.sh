#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EXAMPLE="$ROOT/examples/ci-regression-gate"
BIN="$ROOT/target/debug/verdict"

# Ensure binary is built
cargo build -q --bin verdict

cd "$EXAMPLE"
rm -rf .eval baseline.json

"$BIN" ci \
  --config eval.yaml \
  --trace-file traces/main.jsonl \
  --replay-strict \
  --db .eval/eval.db \
  --export-baseline baseline.json

echo "✅ wrote examples/ci-regression-gate/baseline.json"
