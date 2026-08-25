#!/usr/bin/env bash
# Mutation + fixture battery for the scheduled OSV lockstep/bind contract.
#
# Control must be green. SHA-drift and invocation-bypass must bite the checker.
# The runtime script is executed against synthetic fixtures (scratch only).
# A compare/exit mutation on refuse_if_counts_differ must flip mismatch
# from exit 1 to exit 0, proving the fixture hits that function.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-osv-scanner-scheduled-contract.py"
BIND="scripts/ci/bind-osv-json-sarif-counts.py"
WORKFLOW=".github/workflows/osv-scanner-scheduled.yml"
ACTIVE_INVOKE="python3 scripts/ci/bind-osv-json-sarif-counts.py osv-results.json osv-results.sarif"
COMPARE_LINE="if json_count != sarif_count:"

[[ -f "${ROOT}/${CHECKER}" ]] || { echo "FAIL: checker missing" >&2; exit 1; }
[[ -f "${ROOT}/${BIND}" ]] || { echo "FAIL: bind script missing" >&2; exit 1; }
[[ -f "${ROOT}/${WORKFLOW}" ]] || { echo "FAIL: workflow missing" >&2; exit 1; }

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

seed() {
  local case_root="$1"
  mkdir -p "$case_root/.github/workflows" "$case_root/scripts/ci"
  cp "${ROOT}/${CHECKER}" "$case_root/${CHECKER}"
  cp "${ROOT}/${WORKFLOW}" "$case_root/${WORKFLOW}"
  cp "${ROOT}/${BIND}" "$case_root/${BIND}"
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

run_bind() {
  local name="$1" bind_path="$2" json_path="$3" sarif_path="$4" expected="$5"
  local status=0
  python3 "$bind_path" "$json_path" "$sarif_path" >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

# --- checker cases ---

# 0. Copied real tree is the control.
c="$scratch/control"
seed "$c"
run_checker "control-is-green" "$c" 0

# 1. SHA-drift: roll back ONLY the reporter 40-hex.
c="$scratch/sha-drift"
seed "$c"
python3 - "$c/${WORKFLOW}" <<'PY'
from pathlib import Path
import re
import sys

p = Path(sys.argv[1])
text = p.read_text()
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
run_checker "sha-drift-is-refused" "$c" 1

# 2. Invocation-bypass: replace the exact active invocation with a no-op.
c="$scratch/invoke-bypass"
seed "$c"
python3 - "$c/${WORKFLOW}" "$ACTIVE_INVOKE" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
old = sys.argv[2]
text = p.read_text()
if old not in text:
    raise SystemExit("active invocation missing from seed; cannot mutate")
p.write_text(text.replace(old, "echo 'bypassed'", 1))
PY
run_checker "invoke-bypass-is-refused" "$c" 1

# --- runtime fixtures (scratch only; not added to the repo) ---

fx="$scratch/fixtures"
mkdir -p "$fx"

cat >"$fx/accept.json" <<'JSON'
{"results": [{"vulnerabilities": [{"id": "OSV-TEST-1"}]}]}
JSON
cat >"$fx/accept.sarif" <<'JSON'
{"runs": [{"results": [{"ruleId": "OSV-TEST-1"}]}]}
JSON

cat >"$fx/mismatch.json" <<'JSON'
{"results": [{"vulnerabilities": [{"id": "OSV-TEST-1"}]}]}
JSON
cat >"$fx/mismatch.sarif" <<'JSON'
{"runs": [{"results": []}]}
JSON

printf '{not-json\n' >"$fx/malformed.json"
cat >"$fx/ok.sarif" <<'JSON'
{"runs": [{"results": [{"ruleId": "OSV-TEST-1"}]}]}
JSON

cat >"$fx/ok.json" <<'JSON'
{"results": [{"vulnerabilities": [{"id": "OSV-TEST-1"}]}]}
JSON
cat >"$fx/malformed.sarif" <<'JSON'
{"version": "2.1.0"}
JSON

run_bind "fixture-acceptance" "${ROOT}/${BIND}" "$fx/accept.json" "$fx/accept.sarif" 0
run_bind "fixture-mismatch" "${ROOT}/${BIND}" "$fx/mismatch.json" "$fx/mismatch.sarif" 1
run_bind "fixture-malformed-json" "${ROOT}/${BIND}" "$fx/malformed.json" "$fx/ok.sarif" 1
run_bind "fixture-malformed-sarif" "${ROOT}/${BIND}" "$fx/ok.json" "$fx/malformed.sarif" 1

# --- compare/exit mutation must bite refuse_if_counts_differ ---
# Same mismatch fixture. Unmutated already refused (exit 1 above).
# Mutating `if json_count != sarif_count:` -> `if False:` must accept (exit 0).
mutated="$scratch/mutated-bind.py"
cp "${ROOT}/${BIND}" "$mutated"
python3 - "$mutated" "$COMPARE_LINE" <<'PY'
from pathlib import Path
import sys

p = Path(sys.argv[1])
old = sys.argv[2]
text = p.read_text()
if old not in text:
    raise SystemExit("FAIL: compare/exit string missing from bind script; cannot mutate")
p.write_text(text.replace(old, "if False:", 1))
PY
run_bind "compare-exit-mutation-accepts-mismatch" "$mutated" "$fx/mismatch.json" "$fx/mismatch.sarif" 0

printf 'PASS: osv-scanner scheduled lockstep/bind battery (control, sha-drift, invoke-bypass, fixture-acceptance, fixture-mismatch, fixture-malformed-json, fixture-malformed-sarif, compare-exit-mutation)\n'
