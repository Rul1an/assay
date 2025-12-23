#!/bin/bash
set -e

echo "Build Verdict (Release)..."
cargo build --bin verdict --release --quiet
export PATH=$PWD/target/release:$PATH
export VERDICT=verdict

# Core Tests
echo "running tests/e2e/idempotency.sh..."
bash tests/e2e/idempotency.sh

echo "running tests/e2e/memory_db.sh..."
bash tests/e2e/memory_db.sh

echo "running tests/e2e/edge_cases.sh..."
bash tests/e2e/edge_cases.sh

# Python SDK Tests (Phase 1.x)
echo "running tests/e2e/python_sdk_smoke.sh (Phase 1.0)..."
bash tests/e2e/python_sdk_smoke.sh

echo "running tests/e2e/openai_sdk_smoke.sh (Phase 1.1)..."
bash tests/e2e/openai_sdk_smoke.sh

echo "running tests/e2e/openai_tool_loop_smoke.sh (Phase 1.2)..."
bash tests/e2e/openai_tool_loop_smoke.sh

echo "running tests/e2e/openai_tool_loop_idempotency.sh (Phase 1.2 Idemp)..."
bash tests/e2e/openai_tool_loop_idempotency.sh

echo 'running tests/e2e/openai_streaming_smoke.sh...'; bash tests/e2e/openai_streaming_smoke.sh
echo "ALL E2E TESTS PASSED 🚀"
