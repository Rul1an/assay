#!/usr/bin/env bash
# Mutation battery for the PR OSV Cargo.lock gate.
#
# Control must be green on the live tree. Swapping to the PR-diff reusable,
# floating the pin, widening scan-args, dropping security-events, or deleting
# the cmov rationale must turn the checker red.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-osv-cargo-lock-gate.py"
WORKFLOW=".github/workflows/ci.yml"

[[ -f "${ROOT}/${CHECKER}" ]] || { echo "FAIL: checker missing" >&2; exit 1; }
[[ -f "${ROOT}/${WORKFLOW}" ]] || { echo "FAIL: workflow missing" >&2; exit 1; }

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

seed() {
  local case_root="$1"
  mkdir -p "$case_root/.github/workflows" "$case_root/scripts/ci"
  cp "${ROOT}/${CHECKER}" "$case_root/${CHECKER}"
  cp "${ROOT}/${WORKFLOW}" "$case_root/${WORKFLOW}"
}

run_checker() {
  local name="$1" case_root="$2" expected="$3"
  local status=0
  ( cd "$case_root" && python3 "$CHECKER" ) >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

c="$scratch/control"
seed "$c"
run_checker "control-is-green" "$c" 0

c="$scratch/pr-diff"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "osv-scanner-reusable.yml@",
    "osv-scanner-reusable-pr.yml@",
    1,
)
path.write_text(text)
PY
run_checker "pr-diff-reusable" "$c" 1
if ! grep -F "osv-scanner-reusable-pr.yml" "$scratch/pr-diff-reusable.log" >/dev/null; then
  cat "$scratch/pr-diff-reusable.log" >&2
  echo "FAIL: pr-diff mutation did not name the PR-diff reusable" >&2
  exit 1
fi

c="$scratch/sha-drift"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "a345acffa64b0eaede81a3d9aae6141214d9c8fc",
    "0000000000000000000000000000000000000000",
)
path.write_text(text)
PY
run_checker "sha-drift" "$c" 1

c="$scratch/recursive"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace(
    "        --lockfile=Cargo.lock\n        --lockfile=fuzz/Cargo.lock\n",
    "        --recursive\n        ./\n",
    1,
)
path.write_text(text)
PY
run_checker "recursive-or-dot" "$c" 1

c="$scratch/no-security-events"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
text = text.replace("      security-events: write\n", "", 1)
path.write_text(text)
PY
run_checker "security-events-dropped" "$c" 1

c="$scratch/fail-open"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
needle = "      scan-args: |-\n"
insert = needle + "      fail-on-vuln: false\n"
if needle not in text:
    raise SystemExit("scan-args block missing; cannot insert fail-on-vuln")
path.write_text(text.replace(needle, insert, 1))
PY
run_checker "fail-on-vuln-false" "$c" 1

c="$scratch/rationale-dropped"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
lines = path.read_text().splitlines(keepends=True)
out = []
skip = False
for line in lines:
    if line.startswith("  osv-cargo-lock:"):
        skip = True
        out.append(line)
        continue
    if skip and line.startswith("    #"):
        continue
    skip = False
    out.append(line)
path.write_text("".join(out))
PY
run_checker "rationale-comment-dropped" "$c" 1
if ! grep -F "GHSA-3rjw-m598-pq24" "$scratch/rationale-comment-dropped.log" >/dev/null; then
  cat "$scratch/rationale-comment-dropped.log" >&2
  echo "FAIL: rationale mutation did not name GHSA-3rjw-m598-pq24" >&2
  exit 1
fi

echo "PASS: osv-cargo-lock gate contract (control, pr-diff, sha-drift, recursive, permissions, fail-open, rationale)"
