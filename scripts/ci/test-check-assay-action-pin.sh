#!/usr/bin/env bash
# Mutation battery for the Assay consumer Action pin.
#
# The owner gate is scripts/ci/check-assay-action-pin.sh. Every mutation below
# must fail that same script, then restore from in-process snapshots.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="${ROOT}/scripts/ci/check-assay-action-pin.sh"
READER="${ROOT}/scripts/ci/read-assay-action-pin.sh"
PIN_FILE="${ROOT}/.github/assay-action-pin"
FIXTURE="${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml"
PROVENANCE="${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE"
ASSAY_WF="${ROOT}/.github/workflows/assay.yml"
ACTION_WF="${ROOT}/.github/workflows/action-v2-test.yml"
PRECOMMIT="${ROOT}/.pre-commit-config.yaml"

EXPECTED_PIN="5ba5daf781c229445ea8607060a08770b6f01e14"
EXPECTED_DIGEST="12dd5eab7feb8aef3921b9a39dead7c341d1e019444fced9dcab3b1565b3a1d1"
EXPECTED_USES="Rul1an/assay-action@${EXPECTED_PIN}"

scratch="$(mktemp -d)"
snap="${scratch}/snap"
mkdir -p "${snap}"

LIVE_PATHS=(
  "${PIN_FILE}"
  "${FIXTURE}"
  "${PROVENANCE}"
  "${ASSAY_WF}"
  "${ACTION_WF}"
)

restore_live() {
  local src dest
  for dest in "${LIVE_PATHS[@]}"; do
    src="${snap}/$(printf '%s' "${dest}" | sed 's|/|_|g')"
    if [[ -f "${src}" ]]; then
      cp "${src}" "${dest}"
    fi
  done
}

trap 'restore_live; rm -rf "${scratch}"' EXIT

require_exists() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    echo "missing required path: ${path}" >&2
    exit 1
  fi
}

snapshot_live() {
  local path dest
  for path in "${LIVE_PATHS[@]}"; do
    dest="${snap}/$(printf '%s' "${path}" | sed 's|/|_|g')"
    cp "${path}" "${dest}"
  done
}

run_checker() {
  "${CHECKER}"
}

run_reader() {
  "${READER}"
}

expect_fail() {
  local name="$1"
  local expected="$2"
  shift 2
  if "$@" >"${scratch}/out" 2>"${scratch}/err"; then
    restore_live
    echo "FAIL: ${name} stayed green; expected failure containing: ${expected}" >&2
    exit 1
  fi
  if ! grep -Fq -- "${expected}" "${scratch}/err"; then
    restore_live
    echo "FAIL: ${name} did not contain '${expected}':" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
  echo "ok    ${name} (owner gate failed)"
}

expect_ok() {
  local name="$1"
  shift
  if ! "$@" >"${scratch}/out" 2>"${scratch}/err"; then
    echo "FAIL: ${name} exited non-zero:" >&2
    cat "${scratch}/out" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
  echo "ok    ${name}"
}

mutate_once() {
  local path="$1" old="$2" new="$3"
  python3 - "$path" "$old" "$new" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old, new = sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
count = text.count(old)
if count != 1:
    raise SystemExit(f"mutation subject count is {count}, want 1: {old}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

check_hook_invokes_gate() {
  python3 - "${PRECOMMIT}" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
if "- id: assay-action-consumer-pin" not in text:
    raise SystemExit("pre-commit is missing assay-action-consumer-pin")
block = text.split("- id: assay-action-consumer-pin", 1)[1].split("\n      - id:", 1)[0]
if "scripts/ci/check-assay-action-pin.sh" not in block:
    raise SystemExit("pre-commit hook does not call scripts/ci/check-assay-action-pin.sh")
if "scripts/ci/test-check-assay-action-pin.sh" not in block:
    raise SystemExit("pre-commit hook does not call scripts/ci/test-check-assay-action-pin.sh")
match = re.search(r"^[ \t]*files:[ \t]*(.+)$", block, re.MULTILINE)
if match is None:
    raise SystemExit("assay-action-consumer-pin hook has no files selector")
pattern = match.group(1).strip()
required = (
    ".github/assay-action-pin",
    ".github/workflows/assay.yml",
    ".github/workflows/action-v2-test.yml",
    "scripts/ci/read-assay-action-pin.sh",
    "scripts/ci/check-assay-action-pin.sh",
    "scripts/ci/test-check-assay-action-pin.sh",
    "scripts/ci/fixtures/assay-action-pin/action.yml",
    "scripts/ci/fixtures/assay-action-pin/PROVENANCE",
    ".pre-commit-config.yaml",
)
missing = [path for path in required if re.search(pattern, path) is None]
if missing:
    raise SystemExit(f"assay-action-consumer-pin hook does not trigger for: {', '.join(missing)}")
PY
}

require_exists "${CHECKER}"
require_exists "${READER}"
require_exists "${PIN_FILE}"
require_exists "${FIXTURE}"
require_exists "${PROVENANCE}"
require_exists "${ASSAY_WF}"
require_exists "${ACTION_WF}"
require_exists "${PRECOMMIT}"

snapshot_live

echo "== no-op control =="
expect_ok "control-is-green" run_checker
expect_ok "reader-returns-pin" bash -c "
  pin=\"\$(${READER})\"
  test \"\${pin}\" = '${EXPECTED_PIN}'
"
python3 - "${FIXTURE}" "${EXPECTED_DIGEST}" <<'PY'
import hashlib
import sys
from pathlib import Path

digest = hashlib.sha256(Path(sys.argv[1]).read_bytes()).hexdigest()
if digest != sys.argv[2]:
    raise SystemExit(f"vendored fixture digest {digest} != {sys.argv[2]}")
PY
echo "ok    fixture-digest-is-pinned"

if grep -R -n -- 'uses:[[:space:]]*\./assay-action' "${ASSAY_WF}" "${ACTION_WF}"; then
  echo "FAIL: in-scope workflows still use ./assay-action" >&2
  exit 1
fi
if ! grep -Fq "uses: ${EXPECTED_USES}" "${ASSAY_WF}"; then
  echo "FAIL: assay.yml does not use the literal published pin" >&2
  exit 1
fi
if [[ "$(grep -c "uses: ${EXPECTED_USES}" "${ACTION_WF}")" -lt 1 ]]; then
  echo "FAIL: action-v2-test.yml does not use the literal published pin" >&2
  exit 1
fi
if grep -n 'uses:[[:space:]]*\${{' "${ASSAY_WF}" "${ACTION_WF}"; then
  echo "FAIL: in-scope workflows derive uses from a variable" >&2
  exit 1
fi
echo "ok    live-uses-are-literal-pin"

expect_ok "pre-commit-calls-owner-gate" check_hook_invokes_gate

echo "== nonexistent / non-40 pin =="
printf '%s\n' 'v3.0.2' >"${PIN_FILE}"
expect_fail "non-40-pin" "want exactly one ^[0-9a-f]{40}$ line" run_reader
expect_fail "non-40-pin-owner-gate" "want exactly one ^[0-9a-f]{40}$ line" run_checker
restore_live

printf '%s\n' '5ba5daf781c229445ea8607060a08770b6f01e1' >"${PIN_FILE}"
expect_fail "short-pin" "want exactly one ^[0-9a-f]{40}$ line" run_checker
restore_live

echo "== snippet ref drift =="
mutate_once \
  "${ASSAY_WF}" \
  "uses: ${EXPECTED_USES}" \
  "uses: Rul1an/assay-action@0000000000000000000000000000000000000000"
expect_fail "snippet-ref-drift" "does not equal pin ${EXPECTED_PIN}" run_checker
restore_live

echo "== undeclared with: input =="
mutate_once \
  "${ASSAY_WF}" \
  "          version: \${{ steps.assay_tag.outputs.version }}" \
  "          version: \${{ steps.assay_tag.outputs.version }}
          undeclared_input: true"
expect_fail "undeclared-with-input" "undeclared input 'undeclared_input'" run_checker
restore_live

echo "== local ./assay-action substitution =="
mutate_once \
  "${ACTION_WF}" \
  "      - name: Test action with no bundles
        uses: ${EXPECTED_USES}" \
  "      - name: Test action with no bundles
        uses: ./assay-action"
expect_fail "local-assay-action-substitution" "uses: ./assay-action" run_checker
restore_live

echo "== pinned fixture byte drift =="
python3 - "${FIXTURE}" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[0] ^= 0x01
path.write_bytes(bytes(data))
PY
expect_fail "fixture-byte-drift" "${EXPECTED_DIGEST}" run_checker
restore_live

echo "== no-op control after restore =="
expect_ok "control-stays-green-after-restore" run_checker

echo "assay action consumer pin contract: PASS"
