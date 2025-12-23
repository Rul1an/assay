#!/bin/bash
set -e

echo "Build Verdict (Release)..."
cargo build --bin verdict --release --quiet
export PATH=$PWD/target/release:$PATH
export VERDICT=verdict

echo "running tests/e2e/idempotency.sh..."
bash tests/e2e/idempotency.sh

echo "running tests/e2e/memory_db.sh..."
bash tests/e2e/memory_db.sh

echo "running tests/e2e/edge_cases.sh..."
bash tests/e2e/edge_cases.sh

echo "ALL E2E TESTS PASSED 🚀"
