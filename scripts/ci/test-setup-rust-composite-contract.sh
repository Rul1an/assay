#!/usr/bin/env bash
# Contract for .github/actions/setup-rust and its week8 + wave6 + fuzz-smoke + ADR025 soak + smoke-install assay + Runner Spike SDK + perf family + Split Wave 0 + CI clippy/rustdoc + Kernel Matrix lint caller surfaces.
# Also fail-closed inventory of intentional direct dtolnay/Swatinem exceptions.
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
FUZZ_SMOKE="${ROOT}/.github/workflows/fuzz-smoke.yml"
ADR025="${ROOT}/.github/workflows/adr025-nightly-evidence.yml"
SMOKE_INSTALL="${ROOT}/.github/workflows/smoke-install.yml"
RUNNER_SPIKE="${ROOT}/.github/workflows/runner-spike-sdk.yml"
PERF_MAIN="${ROOT}/.github/workflows/perf_main.yml"
PERF_PR="${ROOT}/.github/workflows/perf_pr.yml"
PERF_NIGHTLY="${ROOT}/.github/workflows/perf_nightly.yml"
SPLIT_WAVE0="${ROOT}/.github/workflows/split-wave0-gates.yml"
CI_YML="${ROOT}/.github/workflows/ci.yml"
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
[[ -f "${FUZZ_SMOKE}" ]] || fail "missing .github/workflows/fuzz-smoke.yml"
[[ -f "${ADR025}" ]] || fail "missing .github/workflows/adr025-nightly-evidence.yml"
[[ -f "${SMOKE_INSTALL}" ]] || fail "missing .github/workflows/smoke-install.yml"
[[ -f "${RUNNER_SPIKE}" ]] || fail "missing .github/workflows/runner-spike-sdk.yml"
[[ -f "${PERF_MAIN}" ]] || fail "missing .github/workflows/perf_main.yml"
[[ -f "${PERF_PR}" ]] || fail "missing .github/workflows/perf_pr.yml"
[[ -f "${PERF_NIGHTLY}" ]] || fail "missing .github/workflows/perf_nightly.yml"
[[ -f "${SPLIT_WAVE0}" ]] || fail "missing .github/workflows/split-wave0-gates.yml"
[[ -f "${CI_YML}" ]] || fail "missing .github/workflows/ci.yml"
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

miri_commands = re.findall(
    r"(?m)^\s+cargo(?:\s+\+nightly)?\s+miri(?:\s|$).*$",
    jobs["miri-registry-smoke"],
)
if len(miri_commands) != 2 or any(
    "cargo +nightly miri" not in command for command in miri_commands
):
    raise SystemExit(
        "miri-registry-smoke: every cargo miri invocation must explicitly select +nightly; "
        f"got {miri_commands}"
    )

proptest_with = setup_rust_with_map(jobs["proptest-cli-smoke"])
if proptest_with != {}:
    raise SystemExit(
        "proptest-cli-smoke: setup-rust with-map must be exactly {} "
        f"(composite defaults); got {proptest_with}"
    )

print(
    f"ok   wave6: setup-rust after checkout for {', '.join(setup_jobs)}; "
    "miri with-map and +nightly invocations exact; proptest with-map empty; no direct pins"
)
PY

python3 - "${FUZZ_SMOKE}" "${ADR025}" "${SMOKE_INSTALL}" "${RUNNER_SPIKE}" "${PERF_MAIN}" "${PERF_PR}" "${PERF_NIGHTLY}" "${SPLIT_WAVE0}" "${CI_YML}" "${KERNEL_MATRIX}" <<'PY' || fail "simple-caller setup-rust contract failed"
import re, sys
from pathlib import Path

fuzz_text = Path(sys.argv[1]).read_text(encoding="utf-8")
adr025_text = Path(sys.argv[2]).read_text(encoding="utf-8")
smoke_text = Path(sys.argv[3]).read_text(encoding="utf-8")
spike_text = Path(sys.argv[4]).read_text(encoding="utf-8")
perf_main_text = Path(sys.argv[5]).read_text(encoding="utf-8")
perf_pr_text = Path(sys.argv[6]).read_text(encoding="utf-8")
perf_nightly_text = Path(sys.argv[7]).read_text(encoding="utf-8")
split_wave0_text = Path(sys.argv[8]).read_text(encoding="utf-8")
ci_text = Path(sys.argv[9]).read_text(encoding="utf-8")
kernel_matrix_text = Path(sys.argv[10]).read_text(encoding="utf-8")


def jobs_by_id(text: str) -> dict[str, str]:
    return {
        job_id: block
        for job_id, block in re.findall(
            r"(?m)^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)",
            text,
            re.S,
        )
    }


# Top-level step sequence (every `^      - ` item), not merely `- uses:` lines —
# a `run:`/`name:` step between checkout and setup-rust must fail this guard.
def top_level_step_bodies(job_block: str) -> list[str]:
    lines = job_block.splitlines()
    bodies: list[str] = []
    i = 0
    while i < len(lines):
        if re.match(r"^      - ", lines[i]):
            start = i
            i += 1
            while i < len(lines) and not re.match(r"^      - ", lines[i]):
                i += 1
            bodies.append("\n".join(lines[start:i]))
        else:
            i += 1
    return bodies


def step_kind(body: str) -> str:
    m = re.match(r"^      - uses:\s*(\S+)", body)
    if not m:
        m = re.search(r"(?m)^        uses:\s*(\S+)", body)
    if m:
        uses = m.group(1).rstrip("/")
        if uses.startswith("actions/checkout@"):
            return "checkout"
        if uses == "./.github/actions/setup-rust":
            return "setup-rust"
        return "other-uses"
    return "other"


def setup_rust_with_map(job_block: str) -> dict[str, str]:
    m = re.search(
        r"(?m)^      - uses:\s*\./\.github/actions/setup-rust\s*\n"
        r"((?:        .*\n)*)",
        job_block,
    )
    if not m:
        # Named step form: `- name:` then `uses:` (ADR025 soak).
        m = re.search(
            r"(?m)^      - name:\s*.*\n"
            r"        uses:\s*\./\.github/actions/setup-rust\s*\n"
            r"((?:        .*\n)*)",
            job_block,
        )
    if not m:
        return {}
    tail = m.group(1)
    if not re.match(r"^        with:\s*$", tail, re.M):
        return {}
    vals: dict[str, str] = {}
    in_with = False
    for line in tail.splitlines():
        if re.match(r"^        with:\s*$", line):
            in_with = True
            continue
        if not in_with:
            continue
        if re.match(r"^\s*(?:#.*)?$", line):
            continue
        km = re.match(r"^          ([A-Za-z0-9_-]+):\s*(.*?)\s*$", line)
        if km:
            vals[km.group(1)] = km.group(2)
            continue
        break
    return vals


def assert_immediate_setup_rust(job_id: str, block: str) -> str:
    bodies = top_level_step_bodies(block)
    kinds = [step_kind(b) for b in bodies]
    setup_idxs = [i for i, k in enumerate(kinds) if k == "setup-rust"]
    if len(setup_idxs) != 1:
        raise SystemExit(f"{job_id}: expected one setup-rust call, found {len(setup_idxs)}")
    checkout_idxs = [i for i, k in enumerate(kinds) if k == "checkout"]
    if not checkout_idxs or setup_idxs[0] != checkout_idxs[0] + 1:
        raise SystemExit(
            f"{job_id}: setup-rust must be the immediate next top-level step after checkout "
            f"(step kinds: {kinds})"
        )
    return bodies[setup_idxs[0]]


def assert_default_setup_rust_caller(wf_label: str, text: str, job_id: str) -> None:
    """Named setup-rust immediately after checkout; composite defaults only; no direct pins."""
    if re.search(r"dtolnay/rust-toolchain@", text):
        raise SystemExit(f"{wf_label} still calls dtolnay/rust-toolchain directly")
    if re.search(r"Swatinem/rust-cache@", text):
        raise SystemExit(f"{wf_label} still calls Swatinem/rust-cache directly")
    jobs = jobs_by_id(text)
    if job_id not in jobs:
        raise SystemExit(f"{wf_label} missing job {job_id}")
    setup = assert_immediate_setup_rust(job_id, jobs[job_id])
    first = setup.splitlines()[0]
    if first != "      - name: Set up Rust toolchain and cache":
        raise SystemExit(
            f"{job_id}: setup step must be named exactly "
            f"'Set up Rust toolchain and cache', got {first!r}"
        )
    # Reject any with: in the actual setup step body (with before/after uses, any keys).
    if re.search(r"(?m)^        with:\s*$", setup):
        raise SystemExit(
            f"{job_id}: setup-rust must not declare a with: map (composite defaults only)"
        )
    # Trailing-slash uses still classify as setup-rust; with-map rejection above still applies.
    if re.search(r"(?m)^\s+uses:\s*(?:dtolnay/rust-toolchain@|Swatinem/rust-cache@)", setup):
        raise SystemExit(f"{job_id}: setup step must not call dtolnay/Swatinem directly")


# --- fuzz-smoke ---
if re.search(r"dtolnay/rust-toolchain@", fuzz_text):
    raise SystemExit("fuzz-smoke still calls dtolnay/rust-toolchain directly")
if re.search(r"Swatinem/rust-cache@", fuzz_text):
    raise SystemExit("fuzz-smoke still calls Swatinem/rust-cache directly")

m = re.search(r"(?m)^  FUZZ_TOOLCHAIN:\s*(\S+)\s*$", fuzz_text)
if not m or m.group(1) != "nightly-2026-07-28":
    raise SystemExit(
        f"FUZZ_TOOLCHAIN must be exactly nightly-2026-07-28, got "
        f"{m.group(1) if m else None!r}"
    )

fuzz_jobs = jobs_by_id(fuzz_text)
if "fuzz-smoke" not in fuzz_jobs:
    raise SystemExit("fuzz-smoke missing job fuzz-smoke")
fuzz_block = fuzz_jobs["fuzz-smoke"]
assert_immediate_setup_rust("fuzz-smoke", fuzz_block)

fuzz_expected = {
    "toolchain": "${{ env.FUZZ_TOOLCHAIN }}",
    "cache-workspaces": "fuzz -> target",
}
got = setup_rust_with_map(fuzz_block)
if got != fuzz_expected:
    raise SystemExit(
        f"fuzz-smoke: setup-rust with-map must be exactly {fuzz_expected}, got {got}"
    )

print(
    "ok   fuzz-smoke: one setup-rust after checkout; with-map exact "
    "(toolchain=${{ env.FUZZ_TOOLCHAIN }}, cache-workspaces=fuzz -> target); "
    "FUZZ_TOOLCHAIN=nightly-2026-07-28; no direct pins"
)

# --- ADR025 soak (default-caller / composite defaults) ---
adr_jobs = jobs_by_id(adr025_text)
for required in ("soak", "readiness", "closure", "otel_bridge"):
    if required not in adr_jobs:
        raise SystemExit(f"adr025 missing job {required}")

assert_default_setup_rust_caller("adr025", adr025_text, "soak")

for job_id in ("readiness", "closure", "otel_bridge"):
    if "./.github/actions/setup-rust" in adr_jobs[job_id]:
        raise SystemExit(f"adr025 job {job_id} must not call setup-rust")

print(
    "ok   adr025: soak one setup-rust after checkout; with-map empty; "
    "no setup-rust in readiness/closure/otel_bridge; no direct pins"
)

# --- Smoke Install assay (default-caller / composite defaults) ---
smoke_jobs = jobs_by_id(smoke_text)
if "assay" not in smoke_jobs:
    raise SystemExit("smoke-install missing job assay")
assert_default_setup_rust_caller("smoke-install", smoke_text, "assay")
for job_id, block in smoke_jobs.items():
    if job_id == "assay":
        continue
    if "./.github/actions/setup-rust" in block:
        raise SystemExit(f"smoke-install: setup-rust only allowed in assay, found in {job_id}")

print(
    "ok   smoke-install: assay one setup-rust after checkout; with-map empty; "
    "no direct pins"
)

# --- Runner Spike SDK (default-caller / composite defaults) ---
assert_default_setup_rust_caller("runner-spike-sdk", spike_text, "sdk-policy-determinism")

print(
    "ok   runner-spike-sdk: sdk-policy-determinism one setup-rust after checkout; "
    "with-map empty; no direct pins"
)


def assert_setup_rust_exact_with(
    wf_label: str, text: str, job_id: str, expected: dict[str, str]
) -> None:
    """Immediate setup-rust after checkout with an exact with-map (reuse parsers)."""
    if re.search(r"dtolnay/rust-toolchain@", text):
        raise SystemExit(f"{wf_label} still calls dtolnay/rust-toolchain directly")
    if re.search(r"Swatinem/rust-cache@", text):
        raise SystemExit(f"{wf_label} still calls Swatinem/rust-cache directly")
    jobs = jobs_by_id(text)
    if job_id not in jobs:
        raise SystemExit(f"{wf_label} missing job {job_id}")
    block = jobs[job_id]
    setup = assert_immediate_setup_rust(job_id, block)
    got = setup_rust_with_map(block)
    if got != expected:
        raise SystemExit(
            f"{job_id}: setup-rust with-map must be exactly {expected}, got {got}"
        )
    if re.search(r"(?m)^\s+uses:\s*(?:dtolnay/rust-toolchain@|Swatinem/rust-cache@)", setup):
        raise SystemExit(f"{job_id}: setup step must not call dtolnay/Swatinem directly")


# --- Perf Main benches (components: rustfmt) ---
assert_setup_rust_exact_with(
    "perf_main", perf_main_text, "benches", {"components": "rustfmt"}
)
print(
    "ok   perf_main: benches one setup-rust after checkout; "
    "with-map exact {components: rustfmt}; no direct pins"
)

# --- Perf PR benches only (components: rustfmt); leave detect job alone ---
perf_pr_jobs = jobs_by_id(perf_pr_text)
if "benches" not in perf_pr_jobs:
    raise SystemExit("perf_pr missing job benches")
assert_setup_rust_exact_with(
    "perf_pr", perf_pr_text, "benches", {"components": "rustfmt"}
)
for job_id, block in perf_pr_jobs.items():
    if job_id == "benches":
        continue
    if "./.github/actions/setup-rust" in block:
        raise SystemExit(f"perf_pr: setup-rust only allowed in benches, found in {job_id}")
print(
    "ok   perf_pr: benches one setup-rust after checkout; "
    "with-map exact {components: rustfmt}; no setup-rust in other jobs; no direct pins"
)

# --- Perf Nightly forensic (default-caller / composite defaults) ---
assert_default_setup_rust_caller("perf_nightly", perf_nightly_text, "forensic")
print(
    "ok   perf_nightly: forensic one setup-rust after checkout; "
    "with-map empty; no direct pins"
)

# --- Split Wave 0: feature-matrix / quality-gates / semver-public only ---
split_wave0_jobs = jobs_by_id(split_wave0_text)
for required in ("feature-matrix", "quality-gates", "semver-public", "detect-changes", "nightly-safety"):
    if required not in split_wave0_jobs:
        raise SystemExit(f"split-wave0 missing job {required}")

assert_default_setup_rust_caller("split-wave0", split_wave0_text, "feature-matrix")
assert_setup_rust_exact_with(
    "split-wave0", split_wave0_text, "quality-gates", {"components": "clippy"}
)
assert_default_setup_rust_caller("split-wave0", split_wave0_text, "semver-public")

allowed_setup = {"feature-matrix", "quality-gates", "semver-public"}
for job_id, block in split_wave0_jobs.items():
    if job_id in allowed_setup:
        continue
    if "./.github/actions/setup-rust" in block:
        raise SystemExit(
            f"split-wave0: setup-rust only allowed in "
            f"feature-matrix/quality-gates/semver-public, found in {job_id}"
        )

print(
    "ok   split-wave0: feature-matrix + semver-public default setup-rust after checkout; "
    "quality-gates with-map exact {components: clippy}; "
    "no setup-rust in detect-changes/nightly-safety; no direct pins"
)


def assert_job_scoped_setup_rust(
    job_id: str, block: str, expected: dict[str, str]
) -> None:
    """Adjacency, exact with-map, and no direct pins inside this job block only."""
    if re.search(r"dtolnay/rust-toolchain@", block):
        raise SystemExit(f"{job_id}: still calls dtolnay/rust-toolchain directly")
    if re.search(r"Swatinem/rust-cache@", block):
        raise SystemExit(f"{job_id}: still calls Swatinem/rust-cache directly")
    setup = assert_immediate_setup_rust(job_id, block)
    got = setup_rust_with_map(block)
    if got != expected:
        raise SystemExit(
            f"{job_id}: setup-rust with-map must be exactly {expected}, got {got}"
        )
    if expected == {}:
        first = setup.splitlines()[0]
        if first != "      - name: Set up Rust toolchain and cache":
            raise SystemExit(
                f"{job_id}: setup step must be named exactly "
                f"'Set up Rust toolchain and cache', got {first!r}"
            )
        if re.search(r"(?m)^        with:\s*$", setup):
            raise SystemExit(
                f"{job_id}: setup-rust must not declare a with: map (composite defaults only)"
            )
    if re.search(
        r"(?m)^\s+uses:\s*(?:dtolnay/rust-toolchain@|Swatinem/rust-cache@)", setup
    ):
        raise SystemExit(f"{job_id}: setup step must not call dtolnay/Swatinem directly")


# --- CI: clippy / rustdoc only (job-scoped; other jobs keep direct pins) ---
ci_jobs = jobs_by_id(ci_text)
for required in ("clippy", "rustdoc"):
    if required not in ci_jobs:
        raise SystemExit(f"ci.yml missing job {required}")

assert_job_scoped_setup_rust("clippy", ci_jobs["clippy"], {"components": "clippy"})
assert_job_scoped_setup_rust("rustdoc", ci_jobs["rustdoc"], {})

allowed_ci_setup = {"clippy", "rustdoc"}
for job_id, block in ci_jobs.items():
    if job_id in allowed_ci_setup:
        continue
    if "./.github/actions/setup-rust" in block:
        raise SystemExit(
            f"ci.yml: setup-rust only allowed in clippy/rustdoc for this slice, found in {job_id}"
        )

print(
    "ok   ci.yml: clippy with-map exact {components: clippy}; rustdoc empty with-map; "
    "setup-rust only in clippy/rustdoc; job-scoped no direct pins"
)

# --- Kernel Matrix: lint only (build-artifacts keeps direct eBPF toolchain/cache-on-failure) ---
km_jobs = jobs_by_id(kernel_matrix_text)
if "lint" not in km_jobs:
    raise SystemExit("kernel-matrix.yml missing job lint")
if "build-artifacts" not in km_jobs:
    raise SystemExit("kernel-matrix.yml missing job build-artifacts")

assert_job_scoped_setup_rust(
    "lint", km_jobs["lint"], {"components": "rustfmt, clippy"}
)

allowed_km_setup = {"lint"}
for job_id, block in km_jobs.items():
    if job_id in allowed_km_setup:
        continue
    if "./.github/actions/setup-rust" in block:
        raise SystemExit(
            f"kernel-matrix.yml: setup-rust only allowed in lint for this slice, found in {job_id}"
        )

build_block = km_jobs["build-artifacts"]
if not re.search(r"dtolnay/rust-toolchain@", build_block):
    raise SystemExit(
        "build-artifacts: must retain direct dtolnay/rust-toolchain (custom eBPF boundary)"
    )
if "install-ebpf-toolchain.sh" not in build_block:
    raise SystemExit(
        "build-artifacts: must retain pinned eBPF toolchain install script"
    )
if not re.search(r"Swatinem/rust-cache@", build_block):
    raise SystemExit(
        "build-artifacts: must retain direct Swatinem/rust-cache (cache-on-failure boundary)"
    )
if not re.search(r"(?m)^\s*cache-on-failure:\s*true\s*$", build_block):
    raise SystemExit(
        "build-artifacts: must retain Swatinem cache-on-failure: true"
    )

print(
    "ok   kernel-matrix.yml: lint with-map exact {components: rustfmt, clippy}; "
    "setup-rust only in lint; build-artifacts keeps direct toolchain + eBPF + cache-on-failure"
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
    ".github/workflows/fuzz-smoke.yml",
    ".github/workflows/adr025-nightly-evidence.yml",
    ".github/workflows/smoke-install.yml",
    ".github/workflows/runner-spike-sdk.yml",
    ".github/workflows/perf_main.yml",
    ".github/workflows/perf_pr.yml",
    ".github/workflows/perf_nightly.yml",
    ".github/workflows/split-wave0-gates.yml",
    ".github/workflows/ci.yml",
    ".github/workflows/demo.yml",
    ".github/workflows/docs-auto-update.yml",
    ".github/workflows/parity.yml",
    ".github/workflows/release.yml",
)
bad = [p for p in required if paths.count(p) != 1]
if bad:
    raise SystemExit(
        "pull_request.paths must list each trigger path exactly once; "
        + ", ".join(f"{p!r} appears {paths.count(p)} time(s)" for p in bad)
    )
print("ok   kernel-matrix pull_request.paths exact-once for setup-rust + week8 + wave6 + fuzz-smoke + adr025 + smoke-install + runner-spike-sdk + perf_main + perf_pr + perf_nightly + split-wave0 + ci.yml + demo + docs-auto-update + parity + release")
PY


python3 - "${ROOT}" <<'PY' || fail "direct-action exception inventory / pre-commit files regex failed"
import re, sys
from collections import Counter
from pathlib import Path

root = Path(sys.argv[1])
TC = "dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8"
RC = "Swatinem/rust-cache@c19371144df3bb44fab255c43d04cbc2ab54d1c4"

def jobs_by_id(text: str) -> dict[str, str]:
    return {j: b for j, b in re.findall(
        r"(?m)^  ([A-Za-z0-9_-]+):\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)", text, re.S)}

# uses: unquoted / 'single' / "double" (incl. named-step indent)
USES_RE = re.compile(
    r"(?m)^(?:      - uses:|        uses:)\s*(?:'|\")?"
    r"((?:dtolnay/rust-toolchain|Swatinem/rust-cache)@[^\s#'\"]+)(?:'|\")?")

# (workflow, job, pin, reason) — composite cannot preserve named behavior
ALLOWED = [
    (".github/workflows/ci.yml", "public-msrv", TC, "dynamic ASSAY_PUBLIC_MSRV toolchain; no composite Swatinem key"),
    (".github/workflows/ci.yml", "public-msrv", RC, "Swatinem key public-msrv-${{ env.ASSAY_PUBLIC_MSRV }}; no composite key"),
    (".github/workflows/ci.yml", "public-crate-policy", TC, "no-cache policy validation; composite always caches"),
    (".github/workflows/ci.yml", "perf", RC, "Swatinem id/output cache-hit; composite has no step id / fixed order"),
    (".github/workflows/ci.yml", "perf", TC, "paired with direct Swatinem id/output; composite forces toolchain-then-cache"),
    (".github/workflows/ci.yml", "test", TC, "paired with sccache cache-directories + cache-on-failure"),
    (".github/workflows/ci.yml", "test", RC, "Swatinem cache-directories (~/.sccache) + cache-on-failure; composite lacks both"),
    (".github/workflows/ci.yml", "ebpf-smoke-ubuntu", TC, "rust-src + install-ebpf-toolchain.sh; composite would inject rust-cache"),
    (".github/workflows/demo.yml", "regenerate", TC, "no-cache demo rebuild; composite always caches"),
    (".github/workflows/docs-auto-update.yml", "generate-docs", TC, "no-cache docs generation; composite always caches"),
    (".github/workflows/kernel-matrix.yml", "build-artifacts", TC, "host toolchain before install-ebpf-toolchain.sh"),
    (".github/workflows/kernel-matrix.yml", "build-artifacts", RC, "Swatinem cache-on-failure for eBPF builds; composite lacks input"),
    (".github/workflows/parity.yml", "parity", TC, "actions/cache not Swatinem; composite would inject Swatinem"),
    (".github/workflows/parity.yml", "integration-parity", TC, "toolchain without Swatinem; composite always caches"),
    (".github/workflows/release.yml", "build", TC, "dtolnay targets: ${{ matrix.target }}; composite has no targets"),
    (".github/workflows/release.yml", "build-mcp-server-linux", TC, "dtolnay targets: ${{ matrix.target }}; composite has no targets"),
    (".github/workflows/release.yml", "release", TC, "no-cache release SBOM toolchain; composite always caches"),
    (".github/workflows/release.yml", "verify-lsm-blocking", TC, "self-hosted no-cache LSM gate; composite always caches"),
    (".github/workflows/release.yml", "publish-crates", TC, "no-cache crates.io publish; composite always caches"),
]
if any(not r.strip() for *_, r in ALLOWED):
    raise SystemExit("allowlist reason must be non-empty/non-whitespace")
keys = [(w, j, p) for w, j, p, _ in ALLOWED]
if len(keys) != len(set(keys)):
    raise SystemExit("allowlist contains duplicate (workflow, job, ref) entries")

measured = []
wf = root / ".github" / "workflows"
for path in sorted(wf.glob("*.yml")) + sorted(wf.glob("*.yaml")):
    body = re.search(r"(?m)^jobs:\n(.*)\Z", path.read_text(encoding="utf-8"), re.S)
    if not body:
        continue
    rel = str(path.relative_to(root))
    for job_id, block in jobs_by_id(body.group(1)).items():
        measured.extend((rel, job_id, ref) for ref in USES_RE.findall(block))

mc, ac = Counter(measured), Counter(keys)
if mc != ac:
    parts = []
    unreg = sorted(k for k in mc if mc[k] > ac[k])
    miss = sorted(k for k in ac if ac[k] > mc[k])
    if unreg:
        parts.append("unregistered direct actions: " + ", ".join(
            f"{w} job {j} uses {r}" for w, j, r in unreg))
    if miss:
        parts.append("allowlisted but absent: " + ", ".join(
            f"{w} job {j} uses {r}" for w, j, r in miss))
    if not parts:
        drift = [(k, mc[k], ac[k]) for k in sorted(set(mc) | set(ac)) if mc[k] != ac[k]]
        parts.append("count drift: " + ", ".join(
            f"{w}/{j}/{r} measured={m} allowlisted={a}" for (w, j, r), m, a in drift))
    raise SystemExit("; ".join(parts) if parts else "direct-action inventory mismatch")
print(f"ok   direct-action exception inventory: {sum(mc.values())} pinned calls "
      f"across {len({w for w, _, _ in mc})} workflows (fail-closed allowlist)")

pc = (root / ".pre-commit-config.yaml").read_text(encoding="utf-8")
in_hook, files_line = False, None
for line in pc.splitlines():
    if re.search(r"id:\s*setup-rust-composite-contract-self-test\s*$", line):
        in_hook = True
        continue
    if in_hook and re.match(r"^\s+- id:\s*", line):
        break
    if in_hook and (m := re.match(r"^\s+files:\s*(.+)\s*$", line)):
        files_line = m.group(1).strip()
        break
if files_line is None:
    raise SystemExit("setup-rust-composite-contract-self-test files: line missing")
expected = (
    r"^(\.github/actions/setup-rust/action\.yml|"
    r"scripts/ci/test-setup-rust-composite-contract\.sh|"
    r"\.pre-commit-config\.yaml|"
    r"\.github/workflows/[^/]+\.ya?ml)$")
if files_line != expected:
    raise SystemExit(
        f"setup-rust pre-commit files regex must be exactly {expected!r}, got {files_line!r}")
cre = re.compile(files_line)
missed = [s for s in (
    ".github/workflows/demo.yml", ".github/workflows/docs-auto-update.yml",
    ".github/workflows/parity.yml", ".github/workflows/release.yml",
    ".github/workflows/ci.yml", ".github/workflows/kernel-matrix.yml",
) if not cre.search(s)]
if missed:
    raise SystemExit(f"pre-commit files regex must match exception workflows; missed {missed}")
print("ok   setup-rust pre-commit files regex covers action, contract, pre-commit config, and workflows")
PY


echo "setup-rust composite contract: all checks passed"
