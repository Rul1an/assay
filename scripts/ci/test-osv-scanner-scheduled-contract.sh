#!/usr/bin/env bash
# Mutation + fixture battery for the scheduled OSV lockstep/bind contract.
#
# Control must be green. SHA-drift and invocation-bypass must bite the checker.
# The runtime script is executed against synthetic fixtures (scratch only),
# always via cwd-fixed names with zero CLI path args.
# A compare/exit mutation on refuse_if_counts_differ must flip mismatch
# from exit 1 to exit 0, proving the fixture hits that function.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-osv-scanner-scheduled-contract.py"
BIND="scripts/ci/bind-osv-json-sarif-counts.py"
WORKFLOW=".github/workflows/osv-scanner-scheduled.yml"
ACTIVE_INVOKE="python3 scripts/ci/bind-osv-json-sarif-counts.py"
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

# Production CLI is argumentless. Fixtures live in cwd as the fixed names.
run_bind_cwd() {
  local name="$1" case_dir="$2" bind_path="$3" expected="$4"
  local status=0
  ( cd "$case_dir" && python3 "$bind_path" ) >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

write_pair() {
  local dest="$1" json_body="$2" sarif_body="$3"
  mkdir -p "$dest"
  printf '%s\n' "$json_body" >"$dest/osv-results.json"
  printf '%s\n' "$sarif_body" >"$dest/osv-results.sarif"
}

# --- checker cases ---

c="$scratch/control"
seed "$c"
run_checker "control-is-green" "$c" 0

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

ACCEPT_JSON='{"results": [{"vulnerabilities": [{"id": "OSV-TEST-1"}]}]}'
ACCEPT_SARIF='{"runs": [{"results": [{"ruleId": "OSV-TEST-1"}]}]}'
EMPTY_SARIF='{"runs": [{"results": []}]}'
MISSING_RUNS_SARIF='{"version": "2.1.0"}'
BAD_JSON='{not-json'

write_pair "$scratch/fx-accept" "$ACCEPT_JSON" "$ACCEPT_SARIF"
write_pair "$scratch/fx-mismatch" "$ACCEPT_JSON" "$EMPTY_SARIF"
write_pair "$scratch/fx-malformed-json" "$BAD_JSON" "$ACCEPT_SARIF"
write_pair "$scratch/fx-malformed-sarif" "$ACCEPT_JSON" "$MISSING_RUNS_SARIF"

run_bind_cwd "fixture-acceptance" "$scratch/fx-accept" "${ROOT}/${BIND}" 0
run_bind_cwd "fixture-mismatch" "$scratch/fx-mismatch" "${ROOT}/${BIND}" 1
run_bind_cwd "fixture-malformed-json" "$scratch/fx-malformed-json" "${ROOT}/${BIND}" 1
run_bind_cwd "fixture-malformed-sarif" "$scratch/fx-malformed-sarif" "${ROOT}/${BIND}" 1

# --- compare/exit mutation must bite refuse_if_counts_differ ---
# Same mismatch fixture (cwd-fixed names, zero CLI args).
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
run_bind_cwd "compare-exit-mutation-accepts-mismatch" "$scratch/fx-mismatch" "$mutated" 0

printf 'PASS: osv-scanner scheduled lockstep/bind battery (control, sha-drift, invoke-bypass, fixture-acceptance, fixture-mismatch, fixture-malformed-json, fixture-malformed-sarif, compare-exit-mutation)\n'
