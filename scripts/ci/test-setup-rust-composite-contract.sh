#!/usr/bin/env bash
# Contract for .github/actions/setup-rust and its week8 + wave6 caller surfaces.
# Guards only the new boundary; actionlint + diff review cover unchanged workflow structure.
#
# Empty optional forwarding is safe for the pinned upstream versions measured in this
# slice: dtolnay/rust-toolchain@29eef... adds no --component flags for empty
# components; Swatinem/rust-cache@c193... treats empty workspaces as its "." default.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ACTION="${ROOT}/.github/actions/setup-rust/action.yml"
WEEK8="${ROOT}/.github/workflows/week8-sota-gates.yml"
WAVE6="${ROOT}/.github/workflows/wave6-nightly-safety.yml"
KERNEL_MATRIX="${ROOT}/.github/workflows/kernel-matrix.yml"
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
[[ -f "${WAVE6}" ]] || fail "missing .github/workflows/wave6-nightly-safety.yml"
[[ -f "${KERNEL_MATRIX}" ]] || fail "missing .github/workflows/kernel-matrix.yml"

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

# Every action@ref (SHA, tag, branch) must be exactly the single wanted pin.
# SHA-only extraction missed mutable refs like @stable beside a correct pin.
for label, action, wanted in (
    ("toolchain", "dtolnay/rust-toolchain", want_toolchain),
    ("cache", "Swatinem/rust-cache", want_cache),
):
    found = re.findall(rf"{re.escape(action)}@[^\s#]+", text)
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

python3 - "${WAVE6}" <<'PY' || fail "wave6 setup-rust caller contract failed"
import re, sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
if re.search(r"dtolnay/rust-toolchain@", text):
    raise SystemExit("wave6 still calls dtolnay/rust-toolchain directly")
if re.search(r"Swatinem/rust-cache@", text):
    raise SystemExit("wave6 still calls Swatinem/rust-cache directly")

jobs = {
    job_id: block
    for job_id, block in re.findall(
        r"(?m)^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)",
        text,
        re.S,
    )
}

required_jobs = ("miri-registry-smoke", "proptest-cli-smoke")
for job_id in required_jobs:
    if job_id not in jobs:
        raise SystemExit(f"wave6 missing job {job_id}")

setup_jobs = []
for job_id in required_jobs:
    block = jobs[job_id]
    uses = re.findall(r"(?m)^      - uses:\s*(\S+)", block)
    setup_idxs = [i for i, u in enumerate(uses) if u.rstrip("/") == "./.github/actions/setup-rust"]
    if len(setup_idxs) != 1:
        raise SystemExit(f"{job_id}: expected one setup-rust call, found {len(setup_idxs)}")
    checkout_idxs = [i for i, u in enumerate(uses) if u.startswith("actions/checkout@")]
    if not checkout_idxs or setup_idxs[0] != checkout_idxs[0] + 1:
        raise SystemExit(f"{job_id}: setup-rust must follow checkout in that job")
    setup_jobs.append(job_id)

# Parse with: for the setup-rust step in a job block (immediately after its uses line).
def setup_rust_with_map(block: str) -> dict[str, str]:
    m = re.search(
        r"(?m)^      - uses:\s*\./\.github/actions/setup-rust\s*\n"
        r"((?:        .*\n)*)",
        block,
    )
    if not m:
        return {}
    tail = m.group(1)
    if not re.match(r"^        with:\s*$", tail, re.M):
        # No with: block (defaults) — or unrelated indented lines that are not with:
        return {}
    # Collect key: value under with: until indentation drops below 10 spaces for keys
    vals: dict[str, str] = {}
    in_with = False
    for line in tail.splitlines():
        if re.match(r"^        with:\s*$", line):
            in_with = True
            continue
        if not in_with:
            continue
        km = re.match(r"^          ([A-Za-z0-9_-]+):\s*(.*?)\s*$", line)
        if km:
            vals[km.group(1)] = km.group(2)
            continue
        # end of with: block
        break
    return vals

miri_with = setup_rust_with_map(jobs["miri-registry-smoke"])
miri_expected = {"toolchain": "nightly", "components": "miri"}
if miri_with != miri_expected:
    raise SystemExit(
        f"miri-registry-smoke: setup-rust with-map must be exactly {miri_expected}, "
        f"got {miri_with}"
    )

proptest_with = setup_rust_with_map(jobs["proptest-cli-smoke"])
if proptest_with != {}:
    raise SystemExit(
        "proptest-cli-smoke: setup-rust with-map must be exactly {} "
        f"(composite defaults); got {proptest_with}"
    )

print(
    f"ok   wave6: setup-rust after checkout for {', '.join(setup_jobs)}; "
    "miri with-map exact; proptest with-map empty; no direct pins"
)
PY

python3 - "${KERNEL_MATRIX}" <<'PY' || fail "kernel-matrix hook-trigger paths missing"
import re, sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
in_pr = in_paths = False
paths: list[str] = []
for line in text.splitlines():
    if re.match(r"^  pull_request:\s*$", line):
        in_pr, in_paths = True, False
        continue
    if in_pr and re.match(r"^  \S", line):
        in_pr = in_paths = False
    if in_pr and re.match(r"^    paths:\s*$", line):
        in_paths = True
        continue
    if in_pr and in_paths and re.match(r"^    \S", line):
        in_paths = False
    if in_paths:
        m = re.match(r'^      - "([^"]+)"\s*(?:#.*)?$', line)
        if m:
            paths.append(m.group(1))

required = (
    ".github/actions/setup-rust/**",
    ".github/workflows/week8-sota-gates.yml",
    ".github/workflows/wave6-nightly-safety.yml",
)
bad = [p for p in required if paths.count(p) != 1]
if bad:
    raise SystemExit(
        "pull_request.paths must list each trigger path exactly once; "
        + ", ".join(f"{p!r} appears {paths.count(p)} time(s)" for p in bad)
    )
print("ok   kernel-matrix pull_request.paths exact-once for setup-rust + week8 + wave6")
PY

echo "setup-rust composite contract: all checks passed"
