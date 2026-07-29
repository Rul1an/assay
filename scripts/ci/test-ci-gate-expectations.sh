#!/usr/bin/env bash
set -euo pipefail

# The `CI` gate decides whether a run is green. It accepted `skipped` unconditionally, so a job
# whose own `if:` had broken reported exactly what a deliberately scoped-out job reports, and the
# gate had no way to tell them apart. It imported two scope outputs and echoed both.
#
# This runs the gate's decision logic against constructed states. Extracting the shell out of the
# workflow to test it would give a second copy that can drift from the one CI executes; instead the
# block is read out of `ci.yml` at the line it lives on and run as-is, so the thing under test is
# the thing that ships.
#
# Each case pins an outcome, not merely "something failed" — a gate that goes red for the wrong
# reason is the failure mode one layer up from the one being fixed.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

# Pull the gate's `run:` body out of the workflow: everything between the "Evaluate required job
# results" step's `run: |` and the end of that block. Indentation-based, which is what YAML gives.
extract_gate() {
  python3 - "$WORKFLOW" <<'PY'
import sys

lines = open(sys.argv[1]).read().splitlines()
start = None
for i, line in enumerate(lines):
    if line.strip() == "- name: Evaluate required job results":
        start = i
        break
if start is None:
    sys.exit("could not find the gate step in ci.yml")

run_at = None
for i in range(start, len(lines)):
    if lines[i].strip() == "run: |":
        run_at = i
        break
if run_at is None:
    sys.exit("the gate step has no `run: |` block")

indent = len(lines[run_at]) - len(lines[run_at].lstrip())
body = []
for line in lines[run_at + 1 :]:
    if line.strip() and (len(line) - len(line.lstrip())) <= indent:
        break
    body.append(line[indent + 2 :] if len(line) > indent + 2 else "")
print("\n".join(body))
PY
}

GATE="$(extract_gate)"
[[ -n "$GATE" ]] || fail "extracted an empty gate body — the workflow shape changed"
grep -q "MCP_REGISTRY_TOUCHED" <<<"$GATE" \
  || fail "the gate does not read mcp_registry_touched; three scope outputs decide whether a job should run"
grep -q "PUBLIC_MSRV_RESULT" <<<"$GATE" \
  || fail "the gate does not inspect the public-msrv result"
grep -q "RUSTDOC_RESULT" <<<"$GATE" \
  || fail "the gate does not inspect the rustdoc result"
if sed -n '/^run_gate()/,/^}/p' "$0" | grep -qE '(PUBLIC_MSRV|RUSTDOC)_RESULT=success'; then
  fail "run_gate must not inject a successful MSRV or rustdoc result into every scenario"
fi
python3 - "$WORKFLOW" <<'PY' || fail "the required CI rollup does not need every lane listed in REQUIRED"
import sys

# A lane wired only into `needs:` still gates; a lane wired only into the triples does not exist to
# the rollup at all. Both halves are checked: this one asserts the `needs:` membership, and the
# `*_RESULT` greps above assert the gate actually reads the result.
REQUIRED = ("public-msrv", "rustdoc")

lines = open(sys.argv[1]).read().splitlines()
for index, line in enumerate(lines):
    if line == "  ci:":
        for candidate in lines[index + 1 : index + 10]:
            if candidate.strip().startswith("needs:"):
                if any(name not in candidate for name in REQUIRED):
                    sys.exit(1)
                sys.exit(0)
        break
sys.exit(1)
PY

# Run the gate under one environment. Echoes the exit code and captured output.
run_gate() {
  local expected="$1" name="$2"
  shift 2
  local out rc=0
  out="$(env "$@" bash -c "$GATE" 2>&1)" || rc=$?
  if [[ "$expected" == "pass" && $rc -ne 0 ]]; then
    echo "$out" >&2
    fail "$name: expected the gate to pass, it exited $rc"
  fi
  if [[ "$expected" == "fail" && $rc -eq 0 ]]; then
    echo "$out" >&2
    fail "$name: expected the gate to fail, it passed"
  fi
  printf '%s' "$out"
}

ok="success"

# A full run with every job green.
run_gate pass "everything green" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false >/dev/null
echo "ok: a complete green run passes"

# A docs-only run: the four code-gated jobs are legitimately scoped out.
run_gate pass "lightweight scoped out" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=true DEPS_SECURITY_RESULT=skipped CLIPPY_RESULT=skipped RUSTDOC_RESULT=skipped \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=skipped TEST_RESULT=skipped \
  PUBLIC_MSRV_RESULT=skipped \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false >/dev/null
echo "ok: a documentation-only run passes with its jobs scoped out"

# The defect: a code-bearing run where a job that should have executed did not. Before this change
# every one of these was green.
for job in DEPS_SECURITY CLIPPY RUSTDOC PUBLIC_MSRV PERF TEST; do
  out="$(run_gate fail "silently skipped $job" \
    SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
    PUBLIC_MSRV_RESULT=$ok \
    DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
    MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
    EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
    "${job}_RESULT=skipped")"
  grep -qi "was skipped, but this run required it" <<<"$out" \
    || fail "$job skipped: the gate failed, but not for the skip — got: $out"
done
echo "ok: a code-gated job that silently did not run fails the gate, and is named"

# The eBPF case the audit called sharpest: the `== 'true'` form disarms on a typo.
out="$(run_gate fail "ebpf required but skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=true EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
grep -q "ebpf-smoke-ubuntu was skipped" <<<"$out" \
  || fail "the ebpf case failed for the wrong reason: $out"
echo "ok: ebpf-smoke-ubuntu skipped while required fails the gate"

# The output the gate did not read at all until now.
out="$(run_gate fail "registry touched but skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=true)"
grep -q "mcp-registry-foundation was skipped" <<<"$out" \
  || fail "the registry case failed for the wrong reason: $out"
echo "ok: mcp-registry-foundation skipped while touched fails the gate"

# Unconditional jobs may never be skipped, whatever the scope says.
for job in DISTRIBUTION_BOUNDARY VENDORED_PACKS; do
  out="$(run_gate fail "unconditional $job skipped" \
    SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=true DEPS_SECURITY_RESULT=skipped CLIPPY_RESULT=skipped RUSTDOC_RESULT=skipped \
    PUBLIC_MSRV_RESULT=skipped \
    DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
    MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=skipped TEST_RESULT=skipped \
    EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
    "${job}_RESULT=skipped")"
  grep -qi "was skipped, but this run required it" <<<"$out" \
    || fail "$job: expected the unconditional-skip message, got: $out"
done
echo "ok: a job with no condition may not be skipped even on a docs-only run"

# Failure still fails, and scope failing takes the basis for every other judgement with it.
run_gate fail "a job failed" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=failure RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false >/dev/null
run_gate fail "scope itself skipped" \
  SCOPE_RESULT=skipped LIGHTWEIGHT_ONLY= DEPS_SECURITY_RESULT=skipped CLIPPY_RESULT=skipped RUSTDOC_RESULT=skipped \
  PUBLIC_MSRV_RESULT=skipped \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=skipped TEST_RESULT=skipped \
  EBPF_SMOKE_REQUIRED= EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED= >/dev/null
echo "ok: an outright failure fails, and a skipped scope fails"

for result in failure ""; do
  out="$(run_gate fail "public-msrv result ${result:-empty}" \
    SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
    PUBLIC_MSRV_RESULT="$result" \
    DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
    MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
    EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
  grep -q "public-msrv" <<<"$out" \
    || fail "public-msrv ${result:-empty}: the gate failed without naming the job"
done
echo "ok: a failed or missing public-msrv result fails closed and names the job"

# An empty scope output is the typo signature: `'' == 'true'` is false, so the job silently never
# runs. Treating empty as "not required" would reproduce the defect through the fix.
out="$(run_gate fail "empty lightweight_only with skipped code jobs" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY= DEPS_SECURITY_RESULT=skipped CLIPPY_RESULT=skipped RUSTDOC_RESULT=skipped \
  PUBLIC_MSRV_RESULT=skipped \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=skipped TEST_RESULT=skipped \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
grep -qi "was skipped, but this run required it" <<<"$out" \
  || fail "an empty lightweight_only must be treated as not-lightweight, got: $out"
echo "ok: an empty lightweight_only is treated as the strict case, not the permissive one"

# The same discipline for the two outputs whose jobs use the `== 'true'` form. Those are the ones
# a typo disarms silently, so empty must not read as "not required" — the first version of this
# gate only got `lightweight_only` right and left both of these failing open, which is the defect
# it was written to close, surviving in the fix.
out="$(run_gate fail "empty ebpf_smoke_required with the job skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED= EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
grep -q "ebpf-smoke-ubuntu was skipped" <<<"$out" \
  || fail "an empty ebpf_smoke_required must be strict, got: $out"

out="$(run_gate fail "empty mcp_registry_touched with the job skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=)"
grep -q "mcp-registry-foundation was skipped" <<<"$out" \
  || fail "an empty mcp_registry_touched must be strict, got: $out"

# A wrong-case value is the same class as empty: GitHub compares these outputs as strings, so
# `TRUE` is not `true` and the job silently never runs.
out="$(run_gate fail "wrong-case ebpf_smoke_required" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=TRUE EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
grep -q "ebpf-smoke-ubuntu was skipped" <<<"$out" \
  || fail "a wrong-case ebpf_smoke_required must be strict, got: $out"
echo "ok: empty or wrong-case == 'true' outputs are strict, so a disarmed job cannot read green"

# And the legitimate relaxations still work: an explicit false scopes the job out.
run_gate pass "explicit false relaxes both" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false >/dev/null
echo "ok: an explicit false still scopes its job out"

echo "PASS: CI gate expectation contract"
