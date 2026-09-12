#!/usr/bin/env bash
# Contract: a stale fuzz/Cargo.lock must fail a required CI context on the PR that causes it.
#
# The staleness assertion (`cargo metadata --locked` against `fuzz/Cargo.toml`) exists in
# `fuzz-smoke.yml`, which is not a required context, so it can find the defect without being
# able to stop it (#2975). The fix puts the same assertion in a job the required `CI` gate
# already waits on, on every PR, independent of any path list.
#
# This pins the wiring, not just the text: the invocation must be an active step in a job
# that is in the `ci` rollup's `needs:` list, judged by its `name|result|expectation` triple,
# and that job must have a Rust toolchain available. Mutants below prove each half red.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() { echo "ok   $*"; }

[[ -f "$WORKFLOW" ]] || fail "missing ${WORKFLOW#"${ROOT}"/}"

SANDBOX_ROOT="$(mktemp -d)"
trap 'rm -rf "$SANDBOX_ROOT"' EXIT

# Print the qualifying job id when the wiring holds; exit 1 otherwise.
# Every assertion takes the workflow path so the mutants below exercise the same logic.
check_workflow() {
  python3 - "$1" <<'PY'
import re
import sys

lines = open(sys.argv[1]).read().splitlines()

# The `ci` rollup's needs: membership list (single-line form, as checked in).
needs = []
for i, line in enumerate(lines):
    if line == "  ci:":
        for follow in lines[i + 1 : i + 6]:
            m = re.match(r"^    needs:\s*\[(?P<items>[^\]]*)\]\s*$", follow)
            if m:
                needs = [w.strip() for w in m.group("items").split(",") if w.strip()]
                break
        break
if not needs:
    sys.exit("the `ci` rollup has no readable `needs:` list")

# Triples judged by the Evaluate step: job -> (var, expectation).
triples = {}
for line in lines:
    m = re.match(r'^\s*"(?P<job>[A-Za-z0-9_-]+)\|\$\{(?P<var>[A-Z][A-Z0-9_]*)\}\|(?P<exp>[^"]*)"', line)
    if m:
        triples[m.group("job")] = (m.group("var"), m.group("exp"))

# Result env bindings: VAR -> needs.job.
bindings = {}
for line in lines:
    m = re.match(
        r"^\s*(?P<var>[A-Z][A-Z0-9_]*):\s*\$\{\{\s*needs\.(?P<job>[A-Za-z0-9_-]+)\.result\s*\}\}\s*$",
        line,
    )
    if m:
        bindings[m.group("var")] = m.group("job")


def job_block(name):
    start = None
    for i, line in enumerate(lines):
        if line == f"  {name}:":
            start = i
            break
    if start is None:
        return []
    end = next(
        (
            i
            for i in range(start + 1, len(lines))
            if re.match(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:\s*(?:#.*)?$", lines[i])
        ),
        len(lines),
    )
    return lines[start:end]


def step_run_blocks(block):
    """Yield (step_name, run_lines) for steps with a `run: |` body."""
    out = []
    i = 0
    while i < len(block):
        m = re.match(r"^      - (?:name:\s*(?P<name>.*?)\s*)?$", block[i])
        if m:
            step_name = (m.group("name") or "<unnamed>").strip()
            # Wide window: comment blocks above `run: |` can be long, and a narrow
            # window would stop seeing the step it already named.
            for j in range(i, min(i + 40, len(block))):
                if block[j].strip() == "run: |":
                    indent = len(block[j]) - len(block[j].lstrip())
                    body = []
                    for k in range(j + 1, len(block)):
                        ln = block[k]
                        if ln.strip() and (len(ln) - len(ln.lstrip())) <= indent:
                            break
                        body.append(ln[indent + 2 :] if len(ln) > indent + 2 else "")
                    out.append((step_name, body))
                    break
        i += 1
    return out


FORBIDDEN_CAUSE_WORDS = ("stale", "cargo update", "out of date", "outdated", "regenerate")

for job in needs:
    # The rollup must judge the job, not merely wait on it.
    if job not in triples:
        continue
    var, _ = triples[job]
    if bindings.get(var) != job:
        continue
    block = job_block(job)
    if not block:
        continue
    text = "\n".join(block)
    # A toolchain must be available: a rust setup action or an explicit selection.
    if not (
        "setup-rust" in text or "dtolnay/rust-toolchain" in text or "RUSTUP_TOOLCHAIN" in text
    ):
        continue
    for step_name, body in step_run_blocks(block):
        active = [ln for ln in body if ln.strip() and not ln.strip().startswith("#")]
        hits = [
            ln
            for ln in active
            if "cargo" in ln and "metadata" in ln and "--locked" in ln and "fuzz" in ln
        ]
        if not hits:
            continue
        # Only stdout may be redirected; Cargo's stderr stays the diagnosis.
        if any("2>" in ln for ln in active):
            sys.exit(f"`{job}` step `{step_name}` redirects stderr away from the log")
        # The wrapper adds exit-code discipline and nothing else: it must not name a
        # cause (`--locked` also fails on network, registry, or toolchain faults).
        joined = "\n".join(active).lower()
        for word in FORBIDDEN_CAUSE_WORDS:
            if word in joined:
                sys.exit(
                    f"`{job}` step `{step_name}` diagnoses a cause "
                    f"(`{word}`) it did not establish"
                )
        if not re.search(r"cargo error|error above|output above", joined):
            sys.exit(f"`{job}` step `{step_name}` must point at Cargo's own error")
        print(job)
        sys.exit(0)

sys.exit("no job in the required `CI` gate asserts the fuzz lock is current")
PY
}

job="$(check_workflow "$WORKFLOW")" \
  || fail "no job in the required CI gate asserts the fuzz lock is current (#2975)"
ok "fuzz-lock assertion wired through required job \`${job}\`"

# Mutants: each must turn red, naming the half that broke. A guard that stays green when
# its wiring is cut is prose, not a gate.
mutant() {
  local name="$1"
  local file="${SANDBOX_ROOT}/mutant.yml"
  cp "$WORKFLOW" "$file"
  shift
  "$@" "$file" "$job"
  if check_workflow "$file" >/dev/null 2>&1; then
    fail "mutant stayed green: ${name}"
  fi
  ok "mutant red: ${name}"
}

comment_out_invocation() {
  # Decoys in comments never execute; the check must not read them.
  python3 - "$1" <<'PY'
import sys

path = sys.argv[1]
out = []
for line in open(path).read().splitlines():
    if "metadata" in line and "--locked" in line and "fuzz" in line and "cargo" in line:
        indent = line[: len(line) - len(line.lstrip())]
        out.append(f"{indent}# {line.strip()}")
    else:
        out.append(line)
open(path, "w").write("\n".join(out) + "\n")
PY
}

drop_from_needs() {
  # Waiting without judging (or neither) detaches the job from the gate.
  python3 - "$1" "$2" <<'PY'
import re
import sys

path, victim = sys.argv[1], sys.argv[2]
out = []
for line in open(path).read().splitlines():
    m = re.match(r"^(    needs:\s*\[)([^\]]*)(\].*)$", line)
    if m:
        items = [w.strip() for w in m.group(2).split(",") if w.strip() != victim]
        line = f"{m.group(1)}{', '.join(items)}{m.group(3)}"
    out.append(line)
open(path, "w").write("\n".join(out) + "\n")
PY
}

drop_triple() {
  # Membership without a triple is waited on and then ignored.
  python3 - "$1" "$2" <<'PY'
import sys

path, victim = sys.argv[1], sys.argv[2]
lines = open(path).read().splitlines()
lines = [ln for ln in lines if f'"{victim}|${{' not in ln]
open(path, "w").write("\n".join(lines) + "\n")
PY
}

strip_toolchain() {
  # An assertion in a job without a toolchain never runs its cargo command.
  python3 - "$1" "$2" <<'PY'
import re
import sys

path, victim = sys.argv[1], sys.argv[2]
lines = open(path).read().splitlines()
start = next(i for i, ln in enumerate(lines) if ln == f"  {victim}:")
end = next(
    (
        i
        for i in range(start + 1, len(lines))
        if re.match(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:\s*(?:#.*)?$", lines[i])
    ),
    len(lines),
)
block = lines[start:end]
block = [
    ln
    for ln in block
    if "setup-rust" not in ln
    and "dtolnay/rust-toolchain" not in ln
    and "RUSTUP_TOOLCHAIN" not in ln
]
open(path, "w").write("\n".join(lines[:start] + block + lines[end:]) + "\n")
PY
}

hide_stderr() {
  # A stderr redirect keeps the step green-shaped while swallowing the diagnosis.
  python3 - "$1" <<'PY'
import sys

path = sys.argv[1]
out = []
for line in open(path).read().splitlines():
    if "metadata" in line and "--locked" in line and "fuzz" in line and "cargo" in line:
        line = line.replace(">/dev/null", ">/dev/null 2>/dev/null")
    out.append(line)
open(path, "w").write("\n".join(out) + "\n")
PY
}

claim_a_cause() {
  # Naming one cause for every `--locked` failure misdiagnoses outages as staleness.
  python3 - "$1" <<'PY'
import sys

path = sys.argv[1]
out = []
for line in open(path).read().splitlines():
    if "::error::" in line and "fuzz metadata" in line:
        line = line.replace("validation failed", "is stale; run cargo update --workspace")
    out.append(line)
open(path, "w").write("\n".join(out) + "\n")
PY
}

mutant "comment-only invocation" comment_out_invocation
mutant "job dropped from needs" drop_from_needs
mutant "triple dropped from gate" drop_triple
mutant "toolchain stripped" strip_toolchain
mutant "stderr redirected" hide_stderr
mutant "cause claimed" claim_a_cause
