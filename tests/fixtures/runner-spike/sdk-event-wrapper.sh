#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <work-dir>" >&2
  exit 64
fi

: "${ASSAY_RUNNER_SDK_EVENT_LOG:?ASSAY_RUNNER_SDK_EVENT_LOG must be set}"
: "${ASSAY_RUNNER_RUN_ID:?ASSAY_RUNNER_RUN_ID must be set}"
: "${ASSAY_RUNNER_SDK_EVENT_SCHEMA:?ASSAY_RUNNER_SDK_EVENT_SCHEMA must be set}"

work_dir=$1
mkdir -p "$work_dir"
printf '%s\n' "sdk wrapper fixture ran" > "$work_dir/sdk-wrapper-ran.txt"

tool_call_id="${ASSAY_RUNNER_SDK_TOOL_CALL_ID:-tc_runner_sdk_001}"

python3 - "$ASSAY_RUNNER_SDK_EVENT_LOG" "$tool_call_id" <<'PY'
import json
import os
import sys
from pathlib import Path

path = Path(sys.argv[1])
tool_call_id = sys.argv[2]
run_id = os.environ["ASSAY_RUNNER_RUN_ID"]
schema = os.environ["ASSAY_RUNNER_SDK_EVENT_SCHEMA"]

events = [
    {
        "schema": schema,
        "run_id": run_id,
        "seq": 0,
        "event_type": "tool_call_started",
        "source": "deterministic-sdk-fixture",
        "sdk_name": "@openai/agents",
        "sdk_version": "0.0.0-fixture",
        "tool_call_id": tool_call_id,
        "tool": "read_file",
    },
    {
        "schema": schema,
        "run_id": run_id,
        "seq": 1,
        "event_type": "tool_call_completed",
        "source": "deterministic-sdk-fixture",
        "sdk_name": "@openai/agents",
        "sdk_version": "0.0.0-fixture",
        "tool_call_id": tool_call_id,
        "tool": "read_file",
    },
    {
        "schema": schema,
        "run_id": run_id,
        "seq": 2,
        "event_type": "run_finished",
        "source": "deterministic-sdk-fixture",
        "sdk_name": "@openai/agents",
        "sdk_version": "0.0.0-fixture",
    },
]

path.write_text(
    "".join(json.dumps(event, separators=(",", ":")) + "\n" for event in events),
    encoding="utf-8",
)
PY
