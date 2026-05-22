#!/usr/bin/env bash
# Gemini Python google-genai second-runtime SDK + policy fixture wrapper.
#
# Mirrors the S5 openai-agents-sdk-policy-agent.sh structurally. The key
# difference is the tool_call_id source: S5 uses a hardcoded id from the
# DeterministicToolCallModel; the Gemini fixture extracts the id from the
# cassette's recorded Gemini API response (FunctionCall.id) so SDK and
# policy bind to the same value per the level-3 stable-identity rule.
#
# Sequence:
#   1. extract FunctionCall.id from the cassette (helper script)
#   2. export ASSAY_RUNNER_SDK_TOOL_CALL_ID to that id
#   3. run the Python fixture (emits three SDK events to the SDK event log)
#   4. sleep for kernel-event phase boundary (analogous to S5 wrapper)
#   5. run the Gemini-specific policy wrapper
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

ROOT="${ASSAY_FIXTURE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)}"
FIXTURE_DIR="$ROOT/tests/fixtures/runner-spike/gemini-google-genai"
FIXTURE_PYTHON="${ASSAY_RUNNER_GEMINI_FIXTURE_PYTHON:-python3}"
FIXTURE_SCRIPT="${ASSAY_RUNNER_GEMINI_FIXTURE_SCRIPT:-$FIXTURE_DIR/fixture.py}"
EXTRACT_SCRIPT="${ASSAY_RUNNER_GEMINI_EXTRACT_SCRIPT:-$FIXTURE_DIR/extract_cassette_tool_call_id.py}"
POLICY_WRAPPER="${ASSAY_RUNNER_GEMINI_POLICY_WRAPPER:-$FIXTURE_DIR/policy-wrapper.sh}"
PYTHON_DEPS="${ASSAY_RUNNER_GEMINI_PYTHONPATH:-$FIXTURE_DIR/.python-deps}"

if ! command -v "$FIXTURE_PYTHON" >/dev/null 2>&1; then
  echo "error: Gemini fixture Python interpreter not found: $FIXTURE_PYTHON" >&2
  echo "hint: install fixture-local deps first:" >&2
  echo "  python3 -m pip install --require-hashes --target $FIXTURE_DIR/.python-deps -r $FIXTURE_DIR/requirements.txt" >&2
  exit 69
fi

if [ -d "$PYTHON_DEPS" ]; then
  export PYTHONPATH="$PYTHON_DEPS${PYTHONPATH:+:$PYTHONPATH}"
fi

if [ ! -f "$FIXTURE_SCRIPT" ]; then
  echo "error: fixture script missing at $FIXTURE_SCRIPT" >&2
  exit 69
fi

if [ ! -f "$EXTRACT_SCRIPT" ]; then
  echo "error: cassette id extractor missing at $EXTRACT_SCRIPT" >&2
  exit 69
fi

if [ ! -x "$POLICY_WRAPPER" ]; then
  echo "error: Gemini policy wrapper missing or not executable at $POLICY_WRAPPER" >&2
  exit 69
fi

# Step 1+2: pull the cassette's FunctionCall.id and export it so the Python
# fixture and the policy wrapper see the same value. The helper exits
# non-zero if the cassette is missing, malformed, or does not contain
# exactly one FunctionCall.id.
ASSAY_RUNNER_SDK_TOOL_CALL_ID="$("$FIXTURE_PYTHON" "$EXTRACT_SCRIPT")"
export ASSAY_RUNNER_SDK_TOOL_CALL_ID

# Step 3: run the Python fixture against the checked-in cassette.
"$FIXTURE_PYTHON" "$FIXTURE_SCRIPT" "$1"

# Step 4: phase-boundary sleep, mirroring the S5 wrapper's approach to
# avoid mixing the Python fixture's kernel signal with the policy
# subprocess's signal under ring-buffer pressure. The bundle does not
# claim timing.
sleep "${ASSAY_RUNNER_PHASE_DRAIN_SLEEP:-1}"

# Step 5: invoke the Gemini policy wrapper with the same tool_call_id.
"$POLICY_WRAPPER" "$1"
