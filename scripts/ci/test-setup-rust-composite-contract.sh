#!/usr/bin/env bash
# Contract for .github/actions/setup-rust and its week8 first-slice callers.
# Guards only the new boundary; actionlint + diff review cover unchanged week8 structure.
#
# Empty optional forwarding is safe for the pinned upstream versions measured in this
# slice: dtolnay/rust-toolchain@29eef... adds no --component flags for empty
# components; Swatinem/rust-cache@c193... treats empty workspaces as its "." default.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ACTION="${ROOT}/.github/actions/setup-rust/action.yml"
WEEK8="${ROOT}/.github/workflows/week8-sota-gates.yml"
TOOLCHAIN_REF="dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8"
CACHE_REF="Swatinem/rust-cache@c19371144df3bb44fab255c43d04cbc2ab54d1c4"

fail() { echo "FAIL: $*" >&2; exit 1; }
ok() { echo "ok   $*"; }

abort_is_failure() {
  local rc="$1"
  [[ "${rc}" -eq 0 ]] || echo "setup-rust composite contract aborted (exit ${rc}); treat as failure" >&2
}
trap 'abort_is_failure "$?"' ERR

[[ -f "${ACTION}" ]] || fail "missing .github/actions/setup-rust/action.yml"
[[ -f "${WEEK8}" ]] || fail "missing .github/workflows/week8-sota-gates.yml"

grep -qE '^[[:space:]]*using:[[:space:]]*composite[[:space:]]*$' "${ACTION}" \
  || fail "action must declare runs.using: composite"
ok "composite action present"

if grep -qE 'actions/checkout@' "${ACTION}"; then
  fail "composite must not checkout"
else
  ok "no checkout in composite"
fi
if grep -qE 'apt-get|apt install|sudo apt' "${ACTION}"; then
  fail "composite must not apt-install"
else
  ok "no apt in composite"
fi

python3 - "${ACTION}" "${TOOLCHAIN_REF}" "${CACHE_REF}" <<'PY' || fail "composite pin/input contract failed"
import re, sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
want_toolchain, want_cache = sys.argv[2], sys.argv[3]

m = re.search(r"(?m)^inputs:\n(.*?)(?=^[a-zA-Z]|\Z)", text, re.S)
if not m:
    raise SystemExit("no inputs: block")
names = re.findall(r"(?m)^  ([A-Za-z0-9_-]+):\s*$", m.group(1))
if names != ["toolchain", "components", "cache-workspaces"]:
    raise SystemExit(f"inputs {names}, expected toolchain/components/cache-workspaces")
if not re.search(r"(?m)^  toolchain:\n(?:.*\n)*?    default:\s*stable\s*$", text):
    raise SystemExit("toolchain default must be stable")
for opt in ("components", "cache-workspaces"):
    if not re.search(rf"(?m)^  {opt}:\n(?:.*\n)*?    default:\s*(\"\"|'')\s*$", text):
        raise SystemExit(f"{opt} default must be empty")

# All action@SHA uses for each name must be exactly the single wanted ref.
for label, action, wanted in (
    ("toolchain", "dtolnay/rust-toolchain", want_toolchain),
    ("cache", "Swatinem/rust-cache", want_cache),
):
    found = re.findall(rf"{re.escape(action)}@[0-9a-f]{{40}}", text)
    if found != [wanted]:
        raise SystemExit(f"{label} refs {found}, expected [{wanted}]")
if not re.search(r"(?m)^\s+components:\s*\$\{\{\s*inputs\.components\s*\}\}\s*$", text):
    raise SystemExit("must pass components: ${{ inputs.components }}")
if not re.search(r"(?m)^\s+workspaces:\s*\$\{\{\s*inputs\.cache-workspaces\s*\}\}\s*$", text):
    raise SystemExit("must pass workspaces: ${{ inputs.cache-workspaces }}")
print("ok   inputs/defaults; one pin each; empty inputs forwarded")
PY

python3 - "${WEEK8}" <<'PY' || fail "week8 setup-rust caller contract failed"
import re, sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
if re.search(r"dtolnay/rust-toolchain@", text):
    raise SystemExit("week8 still calls dtolnay/rust-toolchain directly")
if re.search(r"Swatinem/rust-cache@", text):
    raise SystemExit("week8 still calls Swatinem/rust-cache directly")

jobs = re.findall(
    r"(?m)^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)",
    text,
    re.S,
)
setup_jobs = []
for job_id, block in jobs:
    uses = re.findall(r"(?m)^      - uses:\s*(\S+)", block)
    setup_idxs = [i for i, u in enumerate(uses) if u.rstrip("/") == "./.github/actions/setup-rust"]
    if not setup_idxs:
        continue
    if len(setup_idxs) != 1:
        raise SystemExit(f"{job_id}: expected one setup-rust call, found {len(setup_idxs)}")
    checkout_idxs = [i for i, u in enumerate(uses) if u.startswith("actions/checkout@")]
    if not checkout_idxs or setup_idxs[0] != checkout_idxs[0] + 1:
        raise SystemExit(f"{job_id}: setup-rust must follow checkout in that job")
    setup_jobs.append(job_id)

if len(setup_jobs) != 2:
    raise SystemExit(f"expected exactly two jobs calling setup-rust, found {setup_jobs}")
print(f"ok   week8: two setup-rust calls after checkout ({', '.join(setup_jobs)}); no direct pins")
PY

echo "setup-rust composite contract: all checks passed"
