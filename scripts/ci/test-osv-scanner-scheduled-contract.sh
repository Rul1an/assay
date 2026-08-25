#!/usr/bin/env bash
# Mutation battery for check-osv-scanner-scheduled-contract.py.
#
# Control must be green. Each mutation must bite. A mutation that passes here
# means the lockstep/count-bind policy has a hole of that shape.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-osv-scanner-scheduled-contract.py"
WORKFLOW=".github/workflows/osv-scanner-scheduled.yml"
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

run_case() {
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

# 0. The real tree (copied) is the control.
c="$scratch/control"
seed "$c"
run_case "control-is-green" "$c" 0

# 1. One SHA rolled back: lockstep hole, the closed Dependabot split.
c="$scratch/one-sha"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import re, sys
p = Path(sys.argv[1])
text = p.read_text()
# Flip the reporter pin only. Keep a well-formed 40-hex so the miss is drift, not parse.
text, n = re.subn(
    r"(osv-reporter-action@)[0-9a-f]{40}",
    r"\g<1>9a498708959aeaef5ef730655706c5a1df1edbc2",
    text,
    count=1,
)
if n != 1:
    raise SystemExit("failed to mutate reporter SHA")
p.write_text(text)
PY
run_case "one-sha-rollback-is-refused" "$c" 1

# 2. Count bind bypassed: the silent-empty-SARIF hole.
c="$scratch/bind-bypass"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
text = p.read_text()
old = "osv_json_vuln_count != osv_sarif_result_count"
if old not in text:
    raise SystemExit("bind condition missing from seed; cannot mutate")
p.write_text(text.replace(old, "False", 1))
PY
run_case "count-bind-bypass-is-refused" "$c" 1

printf 'PASS: osv-scanner scheduled lockstep/count-bind mutation battery (3 cases)\n'
