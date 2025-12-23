#!/bin/bash
set -e

# Setup cleanup
cleanup() {
    echo "Cleaning up..."
    rm -rf .venv_smoke dist build verdict_sdk.egg-info
}
trap cleanup EXIT

echo "=================================="
echo "Phase 4.3: Release Smoke Test 📦"
echo "=================================="

# 1. Build Wheel
echo "[1/4] Building Package..."
cd verdict-sdk/python
pip install "build==1.2.2"
python -m build

# 2. Create Fresh Venv
echo "[2/4] Creating Fresh Virtualenv..."
python3 -m venv .venv_smoke
if [ -f ".venv_smoke/bin/activate" ]; then
    source .venv_smoke/bin/activate
else
    echo "Error: Failed to create/activate venv"
    exit 1
fi
pip install --upgrade pip

# 3. Install Wheel with Extras
# Find the wheel file (version agnostic)
WHEEL=$(ls dist/verdict_sdk-*.whl | head -n 1)
echo "Found wheel: $WHEEL"
pip install "$WHEEL[openai]"

# 4. Run Doctor
echo "[3/4] Running Doctor..."
python -m verdict_sdk doctor

# 5. Mini Quickstart Verification
echo "[4/4] Verifying Core Logic (Mini Quickstart)..."
# Create temp workspace
mkdir -p smoke_test_ws
cd smoke_test_ws

# Setup Minimal Config & Trace
echo "version: 1
tests:
  - id: smoke
    prompt: 'test'
    metrics:
      - name: always_true
        threshold: 0.5" > eval.yaml

echo '{"run_id": "smoke", "events": [{"kind": "model", "content": "ok"}]}' > trace.jsonl

# Run Evaluator via Python one-liner
python -c "from verdict_sdk import Evaluator; passed = Evaluator().run('trace.jsonl').passed; print(f'Passed: {passed}'); exit(0) if passed else exit(1)"

echo "✅ Release Smoke Test Passed!"
