#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-sdk-policy-determinism.XXXXXX")"
cleanup() {
  rm -rf -- "$TMP_ROOT"
}
trap cleanup EXIT

WORK_DIR="$TMP_ROOT/work"
RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_sdk_policy_determinism}"
CORRELATION_SCRIPT="${ASSAY_RUNNER_ACCEPTANCE_CORRELATION_SCRIPT:-$ROOT/scripts/ci/runner-spike-sdk-policy-correlation.sh}"
LABEL="${ASSAY_RUNNER_ACCEPTANCE_LABEL:-SDK+policy}"

for run in 1 2 3; do
  echo "=== runner-spike $LABEL determinism run $run/3 ==="
  ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR="$TMP_ROOT/run-$run" \
    ASSAY_RUNNER_ACCEPTANCE_WORK_DIR="$WORK_DIR" \
    ASSAY_RUNNER_ACCEPTANCE_RUN_ID="$RUN_ID" \
    "$CORRELATION_SCRIPT"
done

python3 - "$TMP_ROOT" "$LABEL" <<'PY'
import json
import sys
from pathlib import Path

tmp_root = Path(sys.argv[1])
label = sys.argv[2]


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


baseline_health = read_json(tmp_root / "run-1" / "extract" / "observation-health.json")
baseline_surface = read_json(tmp_root / "run-1" / "extract" / "capability-surface.json")
baseline_correlation = read_json(tmp_root / "run-1" / "extract" / "correlation-report.json")
baseline_sdk = (tmp_root / "run-1" / "extract" / "layers" / "sdk.ndjson").read_text(
    encoding="utf-8"
)
baseline_policy = (tmp_root / "run-1" / "extract" / "layers" / "policy.ndjson").read_text(
    encoding="utf-8"
)

for run in (2, 3):
    health = read_json(tmp_root / f"run-{run}" / "extract" / "observation-health.json")
    surface = read_json(tmp_root / f"run-{run}" / "extract" / "capability-surface.json")
    correlation = read_json(tmp_root / f"run-{run}" / "extract" / "correlation-report.json")
    sdk = (tmp_root / f"run-{run}" / "extract" / "layers" / "sdk.ndjson").read_text(
        encoding="utf-8"
    )
    policy = (tmp_root / f"run-{run}" / "extract" / "layers" / "policy.ndjson").read_text(
        encoding="utf-8"
    )

    if health != baseline_health:
        fail(f"observation-health.json changed between run 1 and run {run}")
    if surface != baseline_surface:
        fail(f"capability-surface.json changed between run 1 and run {run}")
    if correlation != baseline_correlation:
        fail(f"correlation-report.json changed between run 1 and run {run}")
    if sdk != baseline_sdk:
        fail(
            f"layers/sdk.ndjson changed between run 1 and run {run}; "
            "the deterministic SDK fixture should emit byte-stable normalized events."
        )
    if policy != baseline_policy:
        fail(
            f"layers/policy.ndjson changed between run 1 and run {run}; "
            "the deterministic MCP policy fixture should emit byte-stable policy events."
        )

print(f"runner-spike {label} three-run determinism verified")
PY

echo "PASS: runner-spike $LABEL three-run determinism"
