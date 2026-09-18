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

# Every skip assertion states the same two things about one gate run: that it failed *for the
# skip*, and that it named the job under test. The name half is matched with -F against the
# emitted `::error::<name> was skipped` prefix, not as a bare substring: an unanchored match
# accepted a gate that printed `zz-deps-security` while claiming to check `deps-security`, and
# would accept a future job id that merely ends with an existing one.
#
# The two loops call this helper. The single-case sites below whose diagnostic names the scope
# output under test — three of them share one closing `echo`, so a shared message would not say
# which case broke — keep their own wording and match the same prefix inline.
assert_named_skip() {
  local job="$1" out="$2" name
  name="$(printf '%s' "$job" | tr 'A-Z_' 'a-z-')"
  grep -qi "was skipped, but this run required it" <<<"$out" \
    || fail "$job skipped: the gate failed, but not for the skip — got: $out"
  grep -qF -- "::error::${name} was skipped" <<<"$out" \
    || fail "$job skipped: the gate failed for a skip, but did not name $name — got: $out"
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
grep -q "SEMVER_RELEVANT" <<<"$GATE" \
  || fail "the gate does not read semver_relevant from the reusable semver workflow output"
grep -q "SEMVER_PUBLIC_RESULT" <<<"$GATE" \
  || fail "the gate does not read semver_public_result from the reusable semver workflow output"
# Prove reusable workflow output and caller binding:
# 1. semver-public.yml outputs.semver_public_result MUST bind ${{ jobs.conclude.outputs.semver_public_result }}
# 2. semver-public.yml jobs.conclude.outputs.semver_public_result MUST bind ${{ needs.semver-public.result }}
# 3. ci.yml env.SEMVER_PUBLIC_RESULT MUST bind ${{ needs.semver.outputs['semver_public_result'] }}
python3 - "$WORKFLOW" "${ROOT}/.github/workflows/semver-public.yml" <<'PY' \
  || fail "semver_public_result output or caller binding violated the wiring contract"
import sys

ci_path, semver_path = sys.argv[1], sys.argv[2]
ci_lines = open(ci_path).read().splitlines()
semver_lines = open(semver_path).read().splitlines()

def validate_semver_wiring(semver_l: list[str], ci_l: list[str]):
    # 1. semver-public.yml top-level outputs.semver_public_result.value
    found_wf_output = False
    in_outputs = False
    in_semver_public_result = False
    for line in semver_l:
        indent = len(line) - len(line.lstrip())
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if indent == 4 and stripped == "outputs:":
            in_outputs = True
            continue
        if in_outputs:
            if indent <= 4:
                break
            if indent == 6 and stripped == "semver_public_result:":
                in_semver_public_result = True
                continue
            if in_semver_public_result:
                if indent <= 6:
                    in_semver_public_result = False
                elif indent == 8 and stripped.startswith("value:"):
                    expr = stripped[len("value:"):].strip()
                    expr = expr.removeprefix("${{").removesuffix("}}").strip()
                    if expr == "jobs.conclude.outputs.semver_public_result":
                        found_wf_output = True
                    break
    if not found_wf_output:
        raise ValueError("semver-public.yml outputs.semver_public_result does not bind jobs.conclude.outputs.semver_public_result")

    # 2. semver-public.yml conclude job outputs.semver_public_result
    found_conclude_output = False
    in_conclude = False
    for line in semver_l:
        indent = len(line) - len(line.lstrip())
        stripped = line.strip()
        if indent == 2 and stripped == "conclude:":
            in_conclude = True
            continue
        if in_conclude:
            if indent == 2 and stripped != "conclude:":
                break
            if stripped.startswith("semver_public_result:"):
                expr = stripped[len("semver_public_result:"):].strip()
                expr = expr.removeprefix("${{").removesuffix("}}").strip()
                if expr == "needs.semver-public.result":
                    found_conclude_output = True
                break
    if not found_conclude_output:
        raise ValueError("semver-public.yml jobs.conclude.outputs does not bind needs.semver-public.result")

    # 3. Line-by-line bounded parse for ci.yml SEMVER_PUBLIC_RESULT
    found_ci_env = False
    for line in ci_l:
        stripped = line.strip()
        if stripped.startswith("SEMVER_PUBLIC_RESULT:"):
            val = stripped[len("SEMVER_PUBLIC_RESULT:"):].strip()
            val = val.removeprefix("${{").removesuffix("}}").strip()
            if val == "needs.semver.outputs['semver_public_result']":
                found_ci_env = True
                break
    if not found_ci_env:
        raise ValueError("ci.yml does not bind SEMVER_PUBLIC_RESULT to needs.semver.outputs['semver_public_result']")

validate_semver_wiring(semver_lines, ci_lines)

# Bounded mutation checks to prove guard bites
def mutate_lines(lines: list[str], old: str, new: str) -> list[str]:
    return [line.replace(old, new) for line in lines]

mutations = [
    (mutate_lines(semver_lines, "jobs.conclude.outputs.semver_public_result", "'success'"), ci_lines, "literal success in semver-public.yml output"),
    (mutate_lines(semver_lines, "needs.semver-public.result", "'success'"), ci_lines, "literal success in conclude job"),
    (mutate_lines(semver_lines, "needs.semver-public.result", "needs.detect-changes.result"), ci_lines, "wrong job in conclude"),
    (semver_lines, mutate_lines(ci_lines, "needs.semver.outputs['semver_public_result']", "'success'"), "literal success in ci.yml"),
    (semver_lines, mutate_lines(ci_lines, "needs.semver.outputs['semver_public_result']", "needs.semver.outputs['semver_relevant']"), "wrong output in ci.yml"),
]
for mut_semver, mut_ci, desc in mutations:
    try:
        validate_semver_wiring(mut_semver, mut_ci)
    except ValueError:
        continue
    raise SystemExit(f"wiring mutation survived: {desc}")
PY
# Which jobs the gate must wait on and judge is asserted by
# `scripts/ci/check-ci-gate-coverage.py`, derived from the workflow. Three job names used to be
# grepped for here as well; that was a second, hand-maintained statement of the same rule, and a
# hand-maintained list is what let #2230's two jobs sit outside the gate for eleven weeks.
grep -q "write_sha256_sidecar" "$WORKFLOW" \
  || fail "checksum helper changes do not activate the MCP Registry foundation smoke"
python3 - "$WORKFLOW" <<'PY' \
  || fail "the rustdoc lane does not actively cover assay-registry's public OIDC feature"
import sys

required = "cargo doc -p assay-registry --features oidc --no-deps"
lines = open(sys.argv[1]).read().splitlines()


def rustdoc_commands(source):
    start = source.index("  rustdoc:")
    end = next(
        (
            index
            for index in range(start + 1, len(source))
            if source[index].startswith("  ")
            and not source[index].startswith("    ")
            and source[index].endswith(":")
        ),
        len(source),
    )
    return {
        line.strip()
        for line in source[start:end]
        if line.startswith("          ") and not line.lstrip().startswith("#")
    }


if required not in rustdoc_commands(lines):
    sys.exit(1)

# Pin the failure mode: a commented command is documentation, not an executed gate.
mutated = [
    line.replace(required, f"# {required}", 1) if line.strip() == required else line
    for line in lines
]
if required in rustdoc_commands(mutated):
    sys.exit(1)
PY
python3 - "$WORKFLOW" <<'PY' \
  || fail "the release asset contract job does not actively run both contract tests"
import sys

lines = open(sys.argv[1]).read().splitlines()


def job_section(source, name):
    start = source.index(f"  {name}:")
    end = next(
        (
            index
            for index in range(start + 1, len(source))
            if source[index].startswith("  ")
            and not source[index].startswith("    ")
            and source[index].endswith(":")
        ),
        len(source),
    )
    return source[start:end]


def active_commands(source):
    return {
        line.strip()
        for line in job_section(source, "release-asset-contract")
        if line.startswith("          ") and not line.lstrip().startswith("#")
    }


def validate_release_contract(source):
    section = job_section(source, "release-asset-contract")
    commands = active_commands(source)
    if any(command not in commands for command in required):
        raise ValueError("release asset contract command is not active")
    for line in section:
        key = line.strip().split(":", 1)[0]
        if key in {"if", "continue-on-error"}:
            raise ValueError(f"release asset contract may not use {key}")


required = (
    "bash scripts/ci/test-write-sha256-sidecar.sh",
    "bash scripts/ci/test-release-assets.sh",
)
validate_release_contract(lines)

# Pin the no-op failure mode: a commented command is documentation, not an
# executed contract. The independent gate self-test must notice even though
# the release-asset-contract job would still exit successfully.
commented = [
    line.replace(command, f"# {command}", 1) if line.strip() == command else line
    for line in lines
    for command in (required[0],)
]
mutations = [commented]

# A present command can still be skipped or made non-blocking by step/job
# controls. Both are fail-open for a required contract and must be rejected.
step_at = lines.index(
    "      - name: Verify portable checksum production and flat download layout"
)
for control in ("        if: false", "        continue-on-error: true"):
    mutations.append(lines[: step_at + 1] + [control] + lines[step_at + 1 :])

for mutated in mutations:
    try:
        validate_release_contract(mutated)
    except ValueError:
        continue
    raise SystemExit("release asset contract bypass mutation passed")
PY
if sed -n '/^run_gate()/,/^}/p' "$0" | grep -qE '(PUBLIC_MSRV|RUSTDOC)_RESULT=success'; then
  fail "run_gate must not inject a successful MSRV or rustdoc result into every scenario"
fi
# Run the gate under one environment. Echoes the exit code and captured output.
#
# The three results injected here belong to jobs with no `if:`: they always run, `required` is
# their only expectation, and no scope output moves it. A default is therefore not hiding a
# variation, which is why the code-gated lanes below are still written out per scenario. The
# skip cases further down override these explicitly, so the defaults do not make them vacuous.
run_gate() {
  local expected="$1" name="$2"
  shift 2
  local out rc=0 summary
  summary="$(mktemp)"
  out="$(env RELEASE_ASSET_CONTRACT_RESULT=success \
             PUBLISH_SHAPE_CLI_RESULT=success \
             PUBLIC_CRATE_POLICY_RESULT=success \
             GENERATED_DRIFT_RESULT=success \
             EVIDENCEREF_LIVE_RESOLVE_RESULT=success \
             OSV_CARGO_LOCK_RESULT=success \
             SEMVER_RESULT=success \
             SEMVER_PUBLIC_RESULT=success \
             SEMVER_RELEVANT=true \
             SEMVER_OVERRIDE_REASON= \
             SEMVER_OVERRIDE_ACTOR= \
             GITHUB_STEP_SUMMARY="${summary}" \
             "$@" bash -c "$GATE" 2>&1)" || rc=$?
  LAST_GATE_SUMMARY="${summary}"
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
  GENERATED_DRIFT_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success >/dev/null
echo "ok: a complete green run passes"

# A docs-only run: the four code-gated jobs are legitimately scoped out.
run_gate pass "lightweight scoped out" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=true DEPS_SECURITY_RESULT=skipped CLIPPY_RESULT=skipped RUSTDOC_RESULT=skipped \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  GENERATED_DRIFT_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=skipped TEST_RESULT=skipped \
  PUBLIC_MSRV_RESULT=skipped \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=false SEMVER_RESULT=skipped >/dev/null
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
  assert_named_skip "$job" "$out"
done
echo "ok: a code-gated job that silently did not run fails the gate, and is named"

# The eBPF case the audit called sharpest: the `== 'true'` form disarms on a typo.
# A default alone would leave this job in the table and out of every case, which is the shape
# check-ci-gate-coverage.py exists to refuse one level up. This reaches it.
out="$(run_gate fail "evidenceref live-resolve failed" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EVIDENCEREF_LIVE_RESOLVE_RESULT=failure \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
grep -q "evidenceref-live-resolve" <<<"$out" \
  || fail "the gate did not name evidenceref-live-resolve when it failed"
echo "ok: a failing evidenceref-live-resolve fails the gate"

out="$(run_gate fail "ebpf required but skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=true EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
assert_named_skip EBPF_SMOKE_UBUNTU "$out"
echo "ok: ebpf-smoke-ubuntu skipped while required fails the gate"

# The output the gate did not read at all until now.
out="$(run_gate fail "registry touched but skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=true)"
assert_named_skip MCP_REGISTRY_FOUNDATION "$out"
echo "ok: mcp-registry-foundation skipped while touched fails the gate"

# Unconditional jobs may never be skipped, whatever the scope says. `publish-shape-cli` and
# `public-crate-policy` join the list with #2230: both were outside `needs:` entirely, so the gate
# had no opinion about them at all, skipped or failed.
for job in DISTRIBUTION_BOUNDARY VENDORED_PACKS RELEASE_ASSET_CONTRACT PUBLISH_SHAPE_CLI PUBLIC_CRATE_POLICY \
  GENERATED_DRIFT EVIDENCEREF_LIVE_RESOLVE OSV_CARGO_LOCK; do
  out="$(run_gate fail "unconditional $job skipped" \
    SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=true DEPS_SECURITY_RESULT=skipped CLIPPY_RESULT=skipped RUSTDOC_RESULT=skipped \
    PUBLIC_MSRV_RESULT=skipped \
    DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
    MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=skipped TEST_RESULT=skipped \
    EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
    "${job}_RESULT=skipped")"
  assert_named_skip "$job" "$out"
done
echo "ok: a job with no condition may not be skipped even on a docs-only run"

# A red unconditional guardrail must turn the required context red.
for job in PUBLISH_SHAPE_CLI PUBLIC_CRATE_POLICY GENERATED_DRIFT OSV_CARGO_LOCK; do
  out="$(run_gate fail "$job failed" \
    SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
    PUBLIC_MSRV_RESULT=$ok \
    DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
    MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
    EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
    "${job}_RESULT=failure")"
  grep -q "ended with failure" <<<"$out" \
    || fail "$job: expected the gate to report the failed dependency, got: $out"
done
echo "ok: a failed release guardrail fails the required gate"

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

# Semver from reusable workflow: required when relevant=true.
out="$(run_gate fail "semver relevant but skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=skipped)"
assert_named_skip SEMVER "$out"
echo "ok: semver skipped while relevant fails the gate"

# Literal false scopes semver out.
run_gate pass "semver not relevant may skip" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=false SEMVER_RESULT=skipped >/dev/null
echo "ok: semver skipped while not relevant remains green"

# Empty/misspelled semver_relevant is fail-closed, including when semver is skipped.
out="$(run_gate fail "empty semver_relevant with semver skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT= SEMVER_RESULT=skipped)"
grep -q "semver_relevant must be the literal" <<<"$out" \
  || fail "an empty semver_relevant must fail closed, got: $out"
echo "ok: empty semver_relevant fails closed"

# Detection failures in the called workflow fail the CI rollup.
out="$(run_gate fail "semver detection failure surfaces as semver failure" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=failure)"
grep -q "Required CI dependency semver ended with failure" <<<"$out" \
  || fail "a semver detection failure must fail the gate and name semver, got: $out"
echo "ok: semver detection failure fails the gate"

# Override is recorded-only: empty reason does nothing, non-empty reason waives semver failure.
run_gate fail "semver override with empty reason remains failing" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=failure SEMVER_PUBLIC_RESULT=failure SEMVER_OVERRIDE_REASON='   ' SEMVER_OVERRIDE_ACTOR=maintainer >/dev/null
echo "ok: semver override with empty reason does not bypass failure"

run_gate pass "semver override with reason waives semver failure" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=failure SEMVER_PUBLIC_RESULT=failure \
  SEMVER_OVERRIDE_REASON='intentional break before the version bump PR' \
  SEMVER_OVERRIDE_ACTOR='release-maintainer' >/dev/null
grep -q "## Semver override" "${LAST_GATE_SUMMARY}" \
  || fail "override summary heading missing from gate summary"
grep -q "actor: release-maintainer" "${LAST_GATE_SUMMARY}" \
  || fail "override summary must record actor"
grep -q "reason: intentional break before the version bump PR" "${LAST_GATE_SUMMARY}" \
  || fail "override summary must record reason"
echo "ok: semver override with reason records actor+reason and passes"

# Defect #3000: reusable workflow caller reported success, but inner semver-public job skipped or missing.
out="$(run_gate fail "semver relevant, caller success, but inner semver-public skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success SEMVER_PUBLIC_RESULT=skipped)"
grep -q "semver-public was skipped while semver_relevant=true" <<<"$out" \
  || fail "caller success with inner semver-public skipped must fail the gate and name the condition, got: $out"
echo "ok: caller success with inner semver-public skipped fails the gate"

# Never allow skip override when semver_relevant=true!
out="$(run_gate fail "semver override cannot waive skipped inner semver job" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success SEMVER_PUBLIC_RESULT=skipped \
  SEMVER_OVERRIDE_REASON='intentional break before the version bump PR' \
  SEMVER_OVERRIDE_ACTOR='release-maintainer')"
grep -q "semver-public was skipped while semver_relevant=true" <<<"$out" \
  || fail "override cannot waive skipped inner semver-public job, got: $out"
echo "ok: semver override cannot waive skipped inner semver job"

out="$(run_gate fail "semver relevant but inner semver-public result missing" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success SEMVER_PUBLIC_RESULT=)"
grep -q "semver_public_result output" <<<"$out" \
  || fail "missing semver_public_result when semver_relevant=true must fail closed, got: $out"
echo "ok: missing semver_public_result fails closed"

# Defect #3000: caller=success, inner semver-public=failure without override MUST fail!
out="$(run_gate fail "semver relevant, caller success, inner semver-public failure without override" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success SEMVER_PUBLIC_RESULT=failure)"
grep -q "inner semver-public ended with failure" <<<"$out" \
  || fail "inner failure without override must fail the gate and name the condition, got: $out"
echo "ok: inner failure without override fails the gate"

# Cancelled inner job when relevant MUST fail!
out="$(run_gate fail "semver relevant, caller success, inner semver-public cancelled" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success SEMVER_PUBLIC_RESULT=cancelled)"
grep -q "unexpected conclusion 'cancelled'" <<<"$out" \
  || fail "inner cancelled must fail the gate, got: $out"
echo "ok: inner cancelled fails the gate"

# Malformed conclusion when relevant MUST fail!
out="$(run_gate fail "semver relevant, caller success, inner semver-public malformed" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=true SEMVER_RESULT=success SEMVER_PUBLIC_RESULT=bogus)"
grep -q "unexpected conclusion 'bogus'" <<<"$out" \
  || fail "inner bogus conclusion must fail the gate, got: $out"
echo "ok: inner malformed conclusion fails the gate"

# Scope-out pair: relevant=false permits inner skipped
run_gate pass "semver not relevant permits inner skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=false SEMVER_RESULT=skipped SEMVER_PUBLIC_RESULT=skipped >/dev/null
echo "ok: semver not relevant permits inner skipped"

# Scope-out pair: relevant=false permits empty inner result (e.g. detect-changes short-circuit)
run_gate pass "semver not relevant permits empty inner result" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false \
  SEMVER_RELEVANT=false SEMVER_RESULT=skipped SEMVER_PUBLIC_RESULT= >/dev/null
echo "ok: semver not relevant permits empty inner result"

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
grep -qF -- "::error::ebpf-smoke-ubuntu was skipped" <<<"$out" \
  || fail "an empty ebpf_smoke_required must be strict, got: $out"

out="$(run_gate fail "empty mcp_registry_touched with the job skipped" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=skipped PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=false EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=)"
grep -qF -- "::error::mcp-registry-foundation was skipped" <<<"$out" \
  || fail "an empty mcp_registry_touched must be strict, got: $out"

# A wrong-case value is the same class as empty: GitHub compares these outputs as strings, so
# `TRUE` is not `true` and the job silently never runs.
out="$(run_gate fail "wrong-case ebpf_smoke_required" \
  SCOPE_RESULT=$ok LIGHTWEIGHT_ONLY=false DEPS_SECURITY_RESULT=$ok CLIPPY_RESULT=$ok RUSTDOC_RESULT=$ok \
  PUBLIC_MSRV_RESULT=$ok \
  DISTRIBUTION_BOUNDARY_RESULT=$ok VENDORED_PACKS_RESULT=$ok \
  MCP_REGISTRY_FOUNDATION_RESULT=$ok PERF_RESULT=$ok TEST_RESULT=$ok \
  EBPF_SMOKE_REQUIRED=TRUE EBPF_SMOKE_UBUNTU_RESULT=skipped MCP_REGISTRY_TOUCHED=false)"
grep -qF -- "::error::ebpf-smoke-ubuntu was skipped" <<<"$out" \
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
