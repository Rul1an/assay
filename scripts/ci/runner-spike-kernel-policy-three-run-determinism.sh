#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if [ "$(uname -s)" != "Linux" ]; then
  echo "SKIP: runner-spike kernel+policy three-run determinism requires Linux." >&2
  exit 40
fi

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/assay-runner-kernel-policy-determinism.XXXXXX")"
cleanup() {
  rm -rf -- "$TMP_ROOT"
}
trap cleanup EXIT

WORK_DIR="$TMP_ROOT/work"
RUN_ID="run_kernel_policy_determinism"

for run in 1 2 3; do
  echo "=== runner-spike kernel+policy determinism run $run/3 ==="
  ASSAY_RUNNER_ACCEPTANCE_ARTIFACT_DIR="$TMP_ROOT/run-$run" \
    ASSAY_RUNNER_ACCEPTANCE_WORK_DIR="$WORK_DIR" \
    ASSAY_RUNNER_ACCEPTANCE_RUN_ID="$RUN_ID" \
    "$ROOT/scripts/ci/runner-spike-kernel-policy-acceptance.sh"
done

python3 - "$TMP_ROOT" <<'PY'
import json
import sys
from pathlib import Path

tmp_root = Path(sys.argv[1])


def fail(message: str) -> None:
    raise SystemExit(f"FAIL: {message}")


def read_json(path: Path):
    return json.loads(path.read_text(encoding="utf-8"))


baseline_health = read_json(tmp_root / "run-1" / "extract" / "observation-health.json")
baseline_surface = read_json(tmp_root / "run-1" / "extract" / "capability-surface.json")
baseline_correlation = read_json(tmp_root / "run-1" / "extract" / "correlation-report.json")
baseline_policy = (tmp_root / "run-1" / "extract" / "layers" / "policy.ndjson").read_text(
    encoding="utf-8"
)

for run in (2, 3):
    health = read_json(tmp_root / f"run-{run}" / "extract" / "observation-health.json")
    surface = read_json(tmp_root / f"run-{run}" / "extract" / "capability-surface.json")
    correlation = read_json(tmp_root / f"run-{run}" / "extract" / "correlation-report.json")
    policy = (tmp_root / f"run-{run}" / "extract" / "layers" / "policy.ndjson").read_text(
        encoding="utf-8"
    )

    if health != baseline_health:
        fail(
            f"observation-health.json changed between run 1 and run {run}; "
            "this can indicate event_count drift on a noisy host. "
            "Phase 1 requires a clean deterministic run."
        )
    if surface != baseline_surface:
        fail(f"capability-surface.json changed between run 1 and run {run}")
    if correlation != baseline_correlation:
        fail(f"correlation-report.json changed between run 1 and run {run}")
    if policy != baseline_policy:
        fail(f"layers/policy.ndjson changed between run 1 and run {run}")

print("runner-spike kernel+policy three-run determinism verified")
PY

echo "PASS: runner-spike kernel+policy three-run determinism"
