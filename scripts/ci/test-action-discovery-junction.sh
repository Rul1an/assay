#!/usr/bin/env bash
# #2778: pin + published nested sandbox recipe + required default-discovery journey.
# Recipe source: scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd
# Producer: scripts/ci/produce-default-discovery-sandbox-evidence.sh (owned; workflow invokes exactly)
# Pin authority: scripts/ci/read-assay-action-pin.sh (no second hardcoded peel).
# Mutations run only under mktemp; caller workflow/script bytes stay unchanged.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml"
PROVENANCE="${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE"
RECIPE="${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd"
WORKFLOW="${ASSAY_JUNCTION_WORKFLOW:-${ROOT}/.github/workflows/action-v2-test.yml}"
PRODUCER_SH="${ASSAY_JUNCTION_PRODUCER:-${ROOT}/scripts/ci/produce-default-discovery-sandbox-evidence.sh}"
DOC="${ROOT}/docs/guides/github-action.md"
READER="${ROOT}/scripts/ci/read-assay-action-pin.sh"

EXPECTED_PRODUCER_RUN='bash scripts/ci/produce-default-discovery-sandbox-evidence.sh'

die() { echo "action-discovery-junction: $*" >&2; exit 1; }
ok() { echo "ok    $*"; }

# Outer-owned temps for fixture discover (and probes). Full-suite SCRATCH trap must chain this.
JUNCTION_TEMPS=()
register_junction_temp() { JUNCTION_TEMPS+=("$1"); }
cleanup_junction_temps() {
  local p
  for p in "${JUNCTION_TEMPS[@]:-}"; do
    rm -rf "${p}"
  done
  JUNCTION_TEMPS=()
}
trap cleanup_junction_temps EXIT

PIN="$("${READER}")"
[[ "${PIN}" =~ ^[0-9a-f]{40}$ ]] || die "reader pin malformed: ${PIN}"

[[ -f "${RECIPE}" ]] || die "missing single-source recipe ${RECIPE}"
RECIPE_BODY="$(cat "${RECIPE}")"
[[ -n "${RECIPE_BODY}" ]] || die "recipe file is empty"
[[ "${RECIPE_BODY}" == *"assay sandbox"* ]] || die "recipe must invoke assay sandbox"
[[ "${RECIPE_BODY}" == *".assay/evidence/nested/"* ]] || die "recipe must write nested discovery path"
[[ "${RECIPE_BODY}" != *"assay run"* ]] || die "recipe must not teach assay run"

if grep -Fq "Run 'assay run'" "${FIXTURE}"; then
  die "fixture action.yml still teaches Run 'assay run' (stale remediation)"
fi
if grep -Fq "assay run --policy" "${FIXTURE}"; then
  die "fixture action.yml still teaches assay run --policy remediation"
fi
grep -Fq 'remediation_recipe.cmd' "${FIXTURE}" || die "fixture action.yml must load remediation_recipe.cmd"
grep -Eq "^commit=${PIN}$" "${PROVENANCE}" || die "PROVENANCE commit does not equal reader pin"

DOC_TEXT="$(cat "${DOC}")"
[[ "${DOC_TEXT}" == *"${RECIPE_BODY}"* ]] || die "docs/guides/github-action.md must embed remediation_recipe.cmd bytes exactly"
if grep -n "Generate with:" -A8 "${DOC}" | grep -Fq "assay ci"; then
  die "docs troubleshooting still generates evidence via assay ci (stale)"
fi
if grep -Fq -- "--out evidence.tar.gz" "${DOC}"; then
  die "docs still export to evidence.tar.gz outside Action discovery roots"
fi

[[ -f "${PRODUCER_SH}" ]] || die "missing owned producer ${PRODUCER_SH}"
[[ -x "${PRODUCER_SH}" ]] || die "owned producer must be executable: ${PRODUCER_SH}"

python3 - "${WORKFLOW}" "${PIN}" "${PRODUCER_SH}" "${EXPECTED_PRODUCER_RUN}" <<'PY'
import json, re, subprocess, sys
from pathlib import Path

wf_path = Path(sys.argv[1])
pin = sys.argv[2]
producer_sh = Path(sys.argv[3])
expected_producer_run = sys.argv[4]
text = wf_path.read_text(encoding="utf-8")

RUBY = r'''
require "json"
require "yaml"
begin
  val = YAML.safe_load(STDIN.read, aliases: false)
  STDOUT.write(JSON.generate(val))
rescue Psych::SyntaxError, Psych::BadAlias, Psych::DisallowedClass => e
  STDERR.write(e.message)
  exit 2
end
'''

def load_yaml(src: str):
    proc = subprocess.run(
        ["ruby", "-EUTF-8:UTF-8", "-e", RUBY],
        input=src,
        capture_output=True,
        text=True,
        timeout=30,
    )
    if proc.returncode != 0:
        raise SystemExit(f"YAML parse failed: {(proc.stderr or proc.stdout).strip()}")
    return json.loads(proc.stdout)

def step_name(step: dict) -> str:
    name = step.get("name")
    return name.strip() if isinstance(name, str) else ""

def continue_on_error_truthy(step: dict) -> bool:
    """True for boolean true and expression/string forms YAML may yield as str."""
    v = step.get("continue-on-error")
    if v is True:
        return True
    if v is False or v is None:
        return False
    if isinstance(v, (int, float)) and v != 0:
        return True
    if isinstance(v, str):
        s = v.strip().lower()
        if s in ("", "false", "0", "no"):
            return False
        # "true", "yes", "1", or any ${{ ... }} expression — treat as enabling continue
        return True
    return False

def reject_if(label: str, node: dict) -> None:
    if "if" in node:
        raise SystemExit(f"{label} must not set if: (skippable false-green)")

def reject_skippable(label: str, step: dict) -> None:
    reject_if(label, step)
    if continue_on_error_truthy(step):
        raise SystemExit(f"{label} must not set truthy continue-on-error")

def active_shell_lines(src: str) -> list[str]:
    out = []
    for line in src.splitlines():
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        out.append(s)
    return out

def normalize_run(run: str) -> str:
    # Exact body contract: strip one trailing newline for compare stability.
    return run.replace("\r\n", "\n").rstrip("\n")

# --- owned producer: exact effective body (not mere line presence) ---
EXPECTED_PRODUCER_ACTIVE = [
    "set -euo pipefail",
    'ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"',
    'bash -c "$(cat "${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd")"',
    "test -f .assay/evidence/nested/sandbox.tar.gz",
]
producer_text = producer_sh.read_text(encoding="utf-8")
active = active_shell_lines(producer_text)
if active != EXPECTED_PRODUCER_ACTIVE:
    raise SystemExit(
        "owned producer effective body mismatch "
        f"(got {active!r}; want {EXPECTED_PRODUCER_ACTIVE!r})"
    )

data = load_yaml(text)

# Ruby YAML maps the workflow key `on:` to boolean true; JSON.generate then
# stringifies that key as "true". This loader always sees data["true"], never "on".
PRODUCER_PATH_ENTRY = "scripts/ci/produce-default-discovery-sandbox-evidence.sh"
on_node = data.get("true")
if not isinstance(on_node, dict):
    raise SystemExit(
        "workflow on: (parsed as key true) must be a mapping "
        f"(got {type(on_node).__name__})"
    )
push = on_node.get("push")
if not isinstance(push, dict):
    raise SystemExit("workflow on.push must be a mapping")
paths = push.get("paths")
if not isinstance(paths, list) or PRODUCER_PATH_ENTRY not in paths:
    raise SystemExit(
        "on.push.paths must include owned producer script entry "
        f"{PRODUCER_PATH_ENTRY!r} (got {paths!r})"
    )
print("ok    workflow-on-push-paths-include-producer")

jobs = data.get("jobs")
if not isinstance(jobs, dict):
    raise SystemExit("workflow has no jobs mapping")

for sibling in ("test-no-bundles", "required-no-bundles-fails", "corrupt-bundle-refused"):
    if sibling not in jobs:
        raise SystemExit(f"missing DoD sibling job {sibling}")

# optional + absent soft path
nb_job = jobs["test-no-bundles"]
reject_if("test-no-bundles job", nb_job)
nb_uses = [
    s for s in (nb_job.get("steps") or [])
    if isinstance(s, dict) and isinstance(s.get("uses"), str) and "assay-action@" in s["uses"]
]
if len(nb_uses) != 1:
    raise SystemExit("test-no-bundles must have exactly one assay-action uses")
if nb_uses[0]["uses"] != f"Rul1an/assay-action@{pin}":
    raise SystemExit("test-no-bundles uses must track reader pin")
with_nb = nb_uses[0].get("with") or {}
if isinstance(with_nb, dict) and with_nb.get("evidence_mode") == "required":
    raise SystemExit("test-no-bundles must remain optional/default (absent soft path)")

OUTCOME_COMPARE = re.compile(
    r"""\[\s*"\$\{(?P<var>[A-Za-z_][A-Za-z0-9_]*)\}"\s*(?:!=|=|==)\s*"failure"\s*\]"""
)

def require_failure_outcome_binding(job_name: str, action: dict, assertion: dict) -> None:
    aid = action.get("id")
    if not isinstance(aid, str) or not aid.strip():
        raise SystemExit(f"{job_name} Action step must set id: for outcome binding")
    env = assertion.get("env")
    if not isinstance(env, dict):
        raise SystemExit(f"{job_name} assertion must set env: outcome binding")
    expected = f"${{{{ steps.{aid}.outcome }}}}"
    bound_vars = [
        k for k, v in env.items()
        if isinstance(k, str) and isinstance(v, str) and v.strip() == expected
    ]
    if not bound_vars:
        raise SystemExit(
            f"{job_name} assertion must bind an env var to exactly {expected} "
            "(word presence of outcome/failure is not enough)"
        )
    run = assertion.get("run")
    if not isinstance(run, str):
        raise SystemExit(f"{job_name} assertion must have run:")
    matched = False
    for var in bound_vars:
        for m in OUTCOME_COMPARE.finditer(run):
            if m.group("var") == var:
                matched = True
                break
        if matched:
            break
    if not matched:
        raise SystemExit(
            f"{job_name} assertion run must compare the bound outcome env var to "
            '"failure" (e.g. [ "${STEP_OUTCOME}" != "failure" ])'
        )

def require_required_fail_job(job_name: str, assert_name: str) -> None:
    job = jobs[job_name]
    reject_if(f"{job_name} job", job)
    if job.get("timeout-minutes") in (None, 0):
        raise SystemExit(f"{job_name} must set timeout-minutes")
    action = assertion = None
    for s in job.get("steps") or []:
        if not isinstance(s, dict):
            continue
        uses = s.get("uses")
        if isinstance(uses, str) and "assay-action@" in uses:
            action = s
        if step_name(s) == assert_name:
            assertion = s
    if action is None:
        raise SystemExit(f"{job_name} missing Action step")
    if action.get("uses") != f"Rul1an/assay-action@{pin}":
        raise SystemExit(f"{job_name} uses must track reader pin")
    with_val = action.get("with") or {}
    if not isinstance(with_val, dict) or with_val.get("evidence_mode") != "required":
        raise SystemExit(f"{job_name} must set evidence_mode: required")
    if "bundles" in with_val and job_name == "required-no-bundles-fails":
        raise SystemExit(f"{job_name} must omit bundles:")
    if assertion is None or not isinstance(assertion.get("run"), str):
        raise SystemExit(f"{job_name} missing assertion run")
    reject_if(f"{job_name} action", action)
    if not continue_on_error_truthy(action):
        raise SystemExit(
            f"{job_name} Action step must set truthy continue-on-error "
            "(boolean true or string/expression) so the assertion runs"
        )
    reject_skippable(f"{job_name} assertion", assertion)
    # Exact effective assertion body (early exit 0 is not equivalent to regex presence).
    if job_name == "required-no-bundles-fails":
        expected_run = """set -euo pipefail
if [ "${STEP_OUTCOME}" != "failure" ]; then
  echo "ERROR: expected required mode with zero bundles to fail"
  exit 1
fi
if [ "${EVIDENCE_STATE}" = "verified" ]; then
  echo "ERROR: zero bundles must not report verified"
  exit 1
fi
"""
        expected_env = {
            "STEP_OUTCOME": "${{ steps.review.outcome }}",
            "EVIDENCE_STATE": "${{ steps.review.outputs.evidence_state }}",
        }
    else:
        raise SystemExit(f"no exact assertion contract for {job_name}")
    if normalize_run(assertion["run"]) != normalize_run(expected_run):
        raise SystemExit(
            f"{job_name} assertion run must match exact effective body "
            "(dead/early-exit assertion is not OK)"
        )
    aenv = assertion.get("env") if isinstance(assertion.get("env"), dict) else {}
    if aenv != expected_env:
        raise SystemExit(f"{job_name} assertion env must match exactly (got {aenv!r})")
    require_failure_outcome_binding(job_name, action, assertion)

require_required_fail_job(
    "required-no-bundles-fails",
    "Assert required mode fails without bundles",
)

# corrupt job
cj = jobs["corrupt-bundle-refused"]
reject_if("corrupt-bundle-refused job", cj)
if cj.get("timeout-minutes") in (None, 0):
    raise SystemExit("corrupt-bundle-refused must set timeout-minutes")
plant = action = assertion = None
for s in cj.get("steps") or []:
    if not isinstance(s, dict):
        continue
    n = step_name(s)
    if n == "Plant corrupted evidence bundle":
        plant = s
    uses = s.get("uses")
    if isinstance(uses, str) and "assay-action@" in uses:
        action = s
    if n == "Assert corrupted bundle is refused":
        assertion = s
if plant is None or not isinstance(plant.get("run"), str):
    raise SystemExit("corrupt-bundle-refused missing plant step")
pr = plant["run"]
if not any(x in pr for x in ("dd ", "truncate", "corrupt")):
    raise SystemExit("corrupt plant step must actually corrupt bytes")
if action is None or action.get("uses") != f"Rul1an/assay-action@{pin}":
    raise SystemExit("corrupt-bundle-refused uses must track reader pin")
with_c = action.get("with") or {}
if not isinstance(with_c, dict) or with_c.get("evidence_mode") != "required":
    raise SystemExit("corrupt-bundle-refused must set evidence_mode: required")
if assertion is None or not isinstance(assertion.get("run"), str):
    raise SystemExit("corrupt-bundle-refused missing assertion")
reject_skippable("corrupt plant", plant)
reject_if("corrupt action", action)
if not continue_on_error_truthy(action):
    raise SystemExit("corrupt Action step must set truthy continue-on-error")
reject_skippable("corrupt assertion", assertion)
EXPECTED_CORRUPT_ASSERT_RUN = """set -euo pipefail
if [ "${STEP_OUTCOME}" != "failure" ]; then
  echo "ERROR: expected corrupted bundle review to fail"
  exit 1
fi
if [ "${VERIFIED}" = "true" ] || [ "${EVIDENCE_STATE}" = "verified" ]; then
  echo "ERROR: corrupted bundle must not be verified"
  exit 1
fi
"""
EXPECTED_CORRUPT_ASSERT_ENV = {
    "STEP_OUTCOME": "${{ steps.review.outcome }}",
    "EVIDENCE_STATE": "${{ steps.review.outputs.evidence_state }}",
    "VERIFIED": "${{ steps.review.outputs.verified }}",
}
if normalize_run(assertion["run"]) != normalize_run(EXPECTED_CORRUPT_ASSERT_RUN):
    raise SystemExit(
        "corrupt-bundle-refused assertion run must match exact effective body "
        "(dead/early-exit assertion is not OK)"
    )
caenv = assertion.get("env") if isinstance(assertion.get("env"), dict) else {}
if caenv != EXPECTED_CORRUPT_ASSERT_ENV:
    raise SystemExit(f"corrupt assertion env must match exactly (got {caenv!r})")
require_failure_outcome_binding("corrupt-bundle-refused", action, assertion)

job = jobs.get("default-discovery-sandbox-junction")
if not isinstance(job, dict):
    raise SystemExit("missing default-discovery-sandbox-junction job")
reject_if("junction job", job)
if continue_on_error_truthy(job):
    raise SystemExit("junction job must not set truthy continue-on-error")
if job.get("timeout-minutes") in (None, 0):
    raise SystemExit("junction job must set timeout-minutes")

producer = review = assertion = None
for step in job.get("steps") or []:
    if not isinstance(step, dict):
        continue
    n = step_name(step)
    if n == "Produce evidence with published remediation recipe":
        producer = step
    elif n == "Review with required default discovery":
        review = step
    elif n == "Assert verified discovery and nonempty index digest":
        assertion = step

if producer is None or review is None or assertion is None:
    raise SystemExit("junction missing producer/review/assertion step")

for label, step in (("producer", producer), ("review", review), ("assertion", assertion)):
    reject_skippable(f"junction {label}", step)

run = producer.get("run")
if not isinstance(run, str):
    raise SystemExit("producer must have run:")
if normalize_run(run) != expected_producer_run:
    raise SystemExit(
        "producer run body must be exactly the owned-script invoke "
        f"{expected_producer_run!r} (got {normalize_run(run)!r})"
    )

if review.get("uses") != f"Rul1an/assay-action@{pin}":
    raise SystemExit(f"review uses must be Rul1an/assay-action@{pin}")
with_val = review.get("with")
if not isinstance(with_val, dict):
    raise SystemExit("review with: must be a mapping")
if with_val.get("evidence_mode") != "required":
    raise SystemExit("review must set evidence_mode: required")
if "bundles" in with_val:
    raise SystemExit("review must omit bundles: (default discovery only)")

arun = assertion.get("run")
env = assertion.get("env") if isinstance(assertion.get("env"), dict) else {}
joined = arun if isinstance(arun, str) else ""
joined += "\n" + "\n".join(f"{k}={v}" for k, v in env.items())
if not isinstance(arun, str):
    raise SystemExit("assertion must have a run script")
if "verified" not in joined.lower():
    raise SystemExit("assertion must check verified")
if "INDEX_DIGEST" not in joined and "evidence_index_digest" not in joined:
    raise SystemExit("assertion must check evidence_index_digest")
if "nested/sandbox.tar.gz" not in arun:
    raise SystemExit("assertion must pin nested sandbox path")

EXPECTED_ASSERTION_RUN = """set -euo pipefail
test "${EVIDENCE_STATE}" = "verified"
test "${VERIFIED}" = "true"
test -n "${INDEX_DIGEST}"
test -f "${INDEX_PATH}"
python3 -c '
import hashlib, json, sys
path, digest = sys.argv[1], sys.argv[2]
raw = open(path, "rb").read()
assert hashlib.sha256(raw).hexdigest() == digest
data = json.loads(raw)
assert data.get("complete") is True
assert len(data.get("bundles") or []) >= 1
row = data["bundles"][0]
assert row["path"] == ".assay/evidence/nested/sandbox.tar.gz"
assert row["source"] == "discovered"
assert row["integrity"] == "verified"
' "${INDEX_PATH}" "${INDEX_DIGEST}"
"""
if normalize_run(arun) != normalize_run(EXPECTED_ASSERTION_RUN):
    raise SystemExit(
        "junction assertion run body must match exact effective contract "
        "(early exit / reordered checks are not equivalent)"
    )
EXPECTED_ASSERTION_ENV = {
    "EVIDENCE_STATE": "${{ steps.review.outputs.evidence_state }}",
    "VERIFIED": "${{ steps.review.outputs.verified }}",
    "INDEX_DIGEST": "${{ steps.review.outputs.evidence_index_digest }}",
    "INDEX_PATH": "${{ steps.review.outputs.evidence_index_path }}",
}
if env != EXPECTED_ASSERTION_ENV:
    raise SystemExit(f"junction assertion env must match exactly (got {env!r})")

print("ok    junction-job-parsed-contract")
print("ok    sibling-dod-cells")
print("ok    owned-producer-effective-body")
print("ok    junction-assertion-exact-body")
PY

ok "pin-recipe-doc-workflow-junction"

# Behavioral producer: early exit 0 leaves lookalike lines but never runs assay.
run_producer_behavior() {
  local prod="$1"
  local sand out rc
  sand="$(mktemp -d "${TMPDIR:-/tmp}/2778-prod-behav.XXXXXX")"
  mkdir -p "${sand}/bin" "${sand}/work"
  cat >"${sand}/bin/assay" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
marker_dir="${ASSAY_JUNCTION_BEHAVIOR_DIR:?}"
: >"${marker_dir}/assay-called"
bundle=""
prev=""
for a in "$@"; do
  if [[ "${prev}" == "--bundle" ]]; then
    bundle="${a}"
  fi
  prev="${a}"
done
[[ -n "${bundle}" ]] || exit 1
mkdir -p "$(dirname "${bundle}")"
printf 'mock-bundle\n' >"${bundle}"
exit 0
MOCK
  chmod +x "${sand}/bin/assay"
  out="${sand}/out.txt"
  rc=0
  (
    cd "${sand}/work"
    export ASSAY_JUNCTION_BEHAVIOR_DIR="${sand}"
    PATH="${sand}/bin:/usr/bin:/bin" bash "${prod}"
  ) >"${out}" 2>&1 || rc=$?
  if [[ "${rc}" -ne 0 ]]; then
    echo "action-discovery-junction: producer behavioral run rc=${rc}" >&2
    cat "${out}" >&2 || true
    rm -rf "${sand}"
    return 1
  fi
  if [[ ! -f "${sand}/assay-called" ]]; then
    echo "action-discovery-junction: producer never invoked assay (non-executed body)" >&2
    rm -rf "${sand}"
    return 1
  fi
  if [[ ! -f "${sand}/work/.assay/evidence/nested/sandbox.tar.gz" ]]; then
    echo "action-discovery-junction: producer did not create nested sandbox bundle" >&2
    rm -rf "${sand}"
    return 1
  fi
  rm -rf "${sand}"
  return 0
}

if ! run_producer_behavior "${PRODUCER_SH}"; then
  die "owned producer failed behavioral execution check"
fi
ok "owned-producer-behavioral"

# Fixture Action discovery execution + pin filter + bounded doc/path contracts.
# Executes the uniquely selected discover run body from the pinned fixture
# action.yml (YAML-parsed). Host-orphan find/compgen demos are not a substitute.
# This covers discovery candidate paths only — not the real indexer or earlier Action steps.
check_explicit_glob_and_pin_filter() {
  local action_yml="${ASSAY_JUNCTION_ACTION_YML:-${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml}"
  local doc_path="${ASSAY_JUNCTION_DOC:-${DOC}}"

  grep -Fq 'if [ -z "$BUNDLES_PATTERN" ]; then' "${action_yml}" \
    || die "pinned action.yml missing empty-bundles find branch"
  grep -Fq 'compgen -G "$BUNDLES_PATTERN"' "${action_yml}" \
    || die "pinned action.yml missing compgen -G explicit branch"
  grep -Fq './.assay/evidence/*.tar.gz' "${action_yml}" \
    || die "pinned action.yml missing find default discovery"

  # Bounded doc contract (not a full prose snapshot).
  grep -Fq 'does **not**' "${doc_path}" \
    || die "docs must state explicit bundles does not (enable globstar)"
  grep -Fq 'enable `globstar`' "${doc_path}" \
    || die "docs must name globstar for explicit bundles"
  grep -Fq 'omit `bundles` instead' "${doc_path}" \
    || die "docs must tell readers to omit bundles for recursive default"
  grep -Fq "attests the sandbox command's observed effects, not that a test suite" "${doc_path}" \
    || die "docs dry-run recipe must qualify observed-effects-only (not suite pass)"

  local glob_scratch capture_default capture_star mock_action run_body pg_runner
  glob_scratch="$(mktemp -d "${TMPDIR:-/tmp}/2802-glob-depth.XXXXXX")"
  register_junction_temp "${glob_scratch}"
  mkdir -p "${glob_scratch}/.assay/evidence/mid/deep"
  : >"${glob_scratch}/.assay/evidence/top.tar.gz"
  : >"${glob_scratch}/.assay/evidence/mid/nested.tar.gz"
  : >"${glob_scratch}/.assay/evidence/mid/deep/deeper.tar.gz"

  mock_action="$(mktemp -d "${TMPDIR:-/tmp}/2802-mock-action.XXXXXX")"
  register_junction_temp "${mock_action}"
  mkdir -p "${mock_action}/scripts"
  capture_default="${glob_scratch}/captured-default.txt"
  capture_star="${glob_scratch}/captured-star.txt"

  # Shared runner: new session/process group; on wall timeout TERM then KILL the group.
  # Descendant cleanup is only claimed where the synthetic child probe below measures it.
  pg_runner="$(mktemp "${TMPDIR:-/tmp}/2802-pg-runner.XXXXXX.py")"
  register_junction_temp "${pg_runner}"
  cat >"${pg_runner}" <<'PYPG'
import json
import os
import signal
import subprocess
import sys
import time


def pids_in_group(pgid: int) -> list:
    """PIDs currently in pgid (macOS/Linux via ps). Empty if none or ps unavailable."""
    try:
        out = subprocess.check_output(
            ["ps", "-axo", "pid=,pgid="],
            text=True,
            stderr=subprocess.DEVNULL,
        )
    except (OSError, subprocess.CalledProcessError):
        return []
    pids = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) < 2:
            continue
        try:
            pid_i, pgid_i = int(parts[0]), int(parts[1])
        except ValueError:
            continue
        if pgid_i == pgid:
            pids.append(pid_i)
    return pids


def group_has_members(pgid: int) -> bool:
    """Group absence is empty membership — not direct-child proc.poll()."""
    return bool(pids_in_group(pgid))


def signal_group(pgid: int, sig: int) -> None:
    """Best-effort group signal; on killpg EPERM/ESRCH fall back to per-pid."""
    try:
        os.killpg(pgid, sig)
        return
    except ProcessLookupError:
        return
    except PermissionError:
        pass
    for pid in pids_in_group(pgid):
        try:
            os.kill(pid, sig)
        except (ProcessLookupError, PermissionError):
            pass


def terminate_process_group(
    proc,
    pgid: int,
    *,
    term_grace_s: float = 2.0,
    kill_grace_s: float = 2.0,
) -> None:
    """TERM the known process group, wait the full grace, then KILL remaining members.

    Direct-child exit (proc.poll() set) is NOT group absence: a TERM-ignoring
    descendant can still be alive. Always escalate with the retained pgid from
    start_new_session; reap the direct child handle separately.
    """
    signal_group(pgid, signal.SIGTERM)

    # Bounded TERM grace — do not return early solely because the leader exited.
    deadline = time.monotonic() + term_grace_s
    while time.monotonic() < deadline:
        if not group_has_members(pgid):
            break
        time.sleep(0.05)

    # Escalate: signal remaining group members even if the session leader is gone.
    if group_has_members(pgid):
        signal_group(pgid, signal.SIGKILL)

    # Reap the direct child handle separately from group membership.
    if proc.poll() is None:
        try:
            proc.wait(timeout=kill_grace_s)
        except subprocess.TimeoutExpired:
            try:
                proc.kill()
            except (ProcessLookupError, PermissionError):
                pass
            try:
                proc.wait(timeout=kill_grace_s)
            except subprocess.TimeoutExpired:
                pass
    else:
        try:
            proc.wait(timeout=0)
        except subprocess.TimeoutExpired:
            pass


def main() -> int:
    if len(sys.argv) < 4:
        print("usage: pg_runner.py TIMEOUT_S CWD ENV_JSON CMD...", file=sys.stderr)
        return 2
    timeout_s = float(sys.argv[1])
    cwd = sys.argv[2]
    env_overlay = json.loads(sys.argv[3])
    cmd = sys.argv[4:]
    env = os.environ.copy()
    env.update({str(k): str(v) for k, v in env_overlay.items()})
    term_grace_s = float(env.get("ASSAY_PG_TERM_GRACE_S", "2.0"))
    kill_grace_s = float(env.get("ASSAY_PG_KILL_GRACE_S", "2.0"))
    proc = subprocess.Popen(
        cmd,
        cwd=cwd,
        env=env,
        start_new_session=True,
    )
    # Retain pgid at spawn (new session leader); do not re-query after leader exit.
    try:
        pgid = os.getpgid(proc.pid)
    except ProcessLookupError:
        pgid = proc.pid
    try:
        proc.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        terminate_process_group(
            proc,
            pgid,
            term_grace_s=term_grace_s,
            kill_grace_s=kill_grace_s,
        )
        print(
            f"fixture discover wall timeout after {timeout_s}s "
            f"(process group TERM/KILL attempted for pgid={pgid})",
            file=sys.stderr,
        )
        return 124
    return int(proc.returncode or 0)


if __name__ == "__main__":
    raise SystemExit(main())
PYPG

  cat >"${mock_action}/scripts/build_evidence_index.sh" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
# Minimal stand-in: record the discover step's raw candidate paths. Not the real indexer.
: "${BUNDLES_FILE:?}"
: "${GITHUB_OUTPUT:?}"
: "${ASSAY_DISCOVERY_CAPTURE:?}"
cp "${BUNDLES_FILE}" "${ASSAY_DISCOVERY_CAPTURE}"
n=0
if [[ -s "${BUNDLES_FILE}" ]]; then
  n="$(grep -c . "${BUNDLES_FILE}" || true)"
fi
{
  echo "count=${n}"
  if [[ "${n}" -gt 0 ]]; then
    echo "found=true"
  else
    echo "found=false"
  fi
} >>"${GITHUB_OUTPUT}"
MOCK
  chmod +x "${mock_action}/scripts/build_evidence_index.sh"

  run_body="$(
    python3 - "${action_yml}" <<'PYDISC'
import json
import subprocess
import sys
from pathlib import Path

RUBY = r"""
require "json"
require "yaml"
begin
  val = YAML.safe_load(STDIN.read, aliases: false)
  STDOUT.write(JSON.generate(val))
rescue Psych::SyntaxError, Psych::BadAlias, Psych::DisallowedClass => e
  STDERR.write(e.message)
  exit 2
end
"""

path = Path(sys.argv[1])
raw = path.read_text(encoding="utf-8")
proc = subprocess.run(
    ["ruby", "-EUTF-8:UTF-8", "-e", RUBY],
    input=raw,
    capture_output=True,
    text=True,
    timeout=30,
)
if proc.returncode != 0:
    raise SystemExit(
        "action.yml YAML parse failed: " + (proc.stderr or proc.stdout).strip()
    )
data = json.loads(proc.stdout)
runs = data.get("runs") if isinstance(data, dict) else None
steps = (runs or {}).get("steps") if isinstance(runs, dict) else None
if not isinstance(steps, list):
    raise SystemExit("action.yml runs.steps missing")
discover = [s for s in steps if isinstance(s, dict) and s.get("id") == "discover"]
if len(discover) == 0:
    raise SystemExit("discover step absent")
if len(discover) > 1:
    raise SystemExit("discover step duplicated")
step = discover[0]
if step.get("shell") != "bash":
    raise SystemExit("discover step shell must be bash")
body = step.get("run")
if not isinstance(body, str) or not body.strip():
    raise SystemExit("discover run body missing")
sys.stdout.write(body)
PYDISC
  )" || die "failed to extract unique fixture discover run body"

  run_fixture_discover() {
    local pattern="$1"
    local capture="$2"
    local ghub_out run_tmp env_json rc
    ghub_out="$(mktemp "${TMPDIR:-/tmp}/2802-ghout.XXXXXX")"
    run_tmp="$(mktemp "${TMPDIR:-/tmp}/2802-discover-run.XXXXXX")"
    register_junction_temp "${ghub_out}"
    register_junction_temp "${run_tmp}"
    printf '%s\n' "${run_body}" >"${run_tmp}"
    mkdir -p "${glob_scratch}/runner-temp"
    # Known fixture env only; no GitHub expression expansion.
    # Wall-clock bound via process-group TERM/KILL (not bare subprocess.run timeout).
    env_json="$(python3 -c 'import json,sys; print(json.dumps({
      "BUNDLES_PATTERN": sys.argv[1],
      "EVIDENCE_MODE": "optional",
      "SANDBOX_BUNDLE": "",
      "RUNNER_TEMP": sys.argv[2],
      "GITHUB_WORKSPACE": sys.argv[3],
      "GITHUB_ACTION_PATH": sys.argv[4],
      "GITHUB_OUTPUT": sys.argv[5],
      "ASSAY_DISCOVERY_CAPTURE": sys.argv[6],
    }))' "${pattern}" "${glob_scratch}/runner-temp" "${glob_scratch}" "${mock_action}" "${ghub_out}" "${capture}")"
    set +e
    python3 "${pg_runner}" 30 "${glob_scratch}" "${env_json}" bash "${run_tmp}"
    rc=$?
    set -e
    rm -f "${run_tmp}" "${ghub_out}"
    [[ "${rc}" -eq 0 ]] || return 1
    return 0
  }

  run_fixture_discover "" "${capture_default}" \
    || die "fixture discover (default) exited non-zero"
  [[ -f "${capture_default}" ]] || die "default discover left no captured candidate list"
  mapfile -t FIND_HITS < <(sort "${capture_default}")
  [[ "${#FIND_HITS[@]}" -eq 3 ]] \
    || die "default fixture discover must hit top+mid+deep (got ${#FIND_HITS[@]}: ${FIND_HITS[*]})"
  [[ "${FIND_HITS[0]}" == "./.assay/evidence/mid/deep/deeper.tar.gz" ]] \
    || die "default missing deeper path (got ${FIND_HITS[*]})"
  [[ "${FIND_HITS[1]}" == "./.assay/evidence/mid/nested.tar.gz" ]] \
    || die "default missing mid path (got ${FIND_HITS[*]})"
  [[ "${FIND_HITS[2]}" == "./.assay/evidence/top.tar.gz" ]] \
    || die "default missing top path (got ${FIND_HITS[*]})"

  run_fixture_discover ".assay/evidence/**/*.tar.gz" "${capture_star}" \
    || die "fixture discover (explicit **) exited non-zero"
  [[ -f "${capture_star}" ]] || die "explicit discover left no captured candidate list"
  mapfile -t STAR_HITS < <(sort "${capture_star}")
  [[ "${#STAR_HITS[@]}" -eq 1 ]] \
    || die "explicit ** without globstar must hit exactly one mid-level path (got ${#STAR_HITS[@]}: ${STAR_HITS[*]})"
  [[ "${STAR_HITS[0]}" == ".assay/evidence/mid/nested.tar.gz" ]] \
    || die "explicit ** hit unexpected path: ${STAR_HITS[0]}"
  for miss in ".assay/evidence/top.tar.gz" ".assay/evidence/mid/deep/deeper.tar.gz" \
              "./.assay/evidence/top.tar.gz" "./.assay/evidence/mid/deep/deeper.tar.gz"; do
    for h in "${STAR_HITS[@]}"; do
      if [[ "${h}" == "${miss}" ]]; then
        die "explicit ** falsely recursive; hit ${miss}"
      fi
    done
  done

  # Positive / no-op control: short script exits 0 under the same process-group runner.
  {
    local noop_sh noop_rc
    noop_sh="$(mktemp "${TMPDIR:-/tmp}/2802-noop.XXXXXX.sh")"
    register_junction_temp "${noop_sh}"
    printf '%s\n' '#!/usr/bin/env bash' 'set -euo pipefail' 'echo noop-ok' >"${noop_sh}"
    set +e
    python3 "${pg_runner}" 5 "${glob_scratch}" '{}' bash "${noop_sh}" >/dev/null
    noop_rc=$?
    set -e
    [[ "${noop_rc}" -eq 0 ]] || die "process-group noop control exited ${noop_rc}"
    ok "process-group-noop-control"
  }

  # Owned-group teardown helper for probes: kill only the recorded child's pgid.
  reap_owned_probe_child() {
    local pid="${1:-}" pgid
    [[ -n "${pid}" ]] || return 0
    if kill -0 "${pid}" 2>/dev/null; then
      pgid="$(ps -o pgid= -p "${pid}" 2>/dev/null | tr -d '[:space:]' || true)"
      if [[ -n "${pgid}" && "${pgid}" =~ ^[0-9]+$ ]]; then
        kill -KILL "-${pgid}" 2>/dev/null || true
      else
        kill -KILL "${pid}" 2>/dev/null || true
      fi
    fi
  }

  # Cooperative synthetic child: group TERM/KILL reaps a normal sleep descendant.
  # Without start_new_session+killpg, subprocess.run(timeout=) leaves this child alive.
  {
    local probe_dir probe_sh child_pid_file alive_rc probe_rc child_pid
    probe_dir="$(mktemp -d "${TMPDIR:-/tmp}/2802-pg-child.XXXXXX")"
    register_junction_temp "${probe_dir}"
    child_pid_file="${probe_dir}/child.pid"
    probe_sh="${probe_dir}/parent.sh"
    cat >"${probe_sh}" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
pidfile="${1:?}"
# Descendant outside the direct bash argv; must share the new session to be reaped.
sleep 120 &
echo $! >"${pidfile}"
# Parent blocks; wall timeout must kill the process group.
wait
PROBE
    chmod +x "${probe_sh}"
    set +e
    python3 "${pg_runner}" 1 "${probe_dir}" '{"ASSAY_PG_TERM_GRACE_S":"0.4","ASSAY_PG_KILL_GRACE_S":"0.4"}' bash "${probe_sh}" "${child_pid_file}" >/dev/null 2>&1
    probe_rc=$?
    set -e
    child_pid=""
    if [[ -f "${child_pid_file}" ]]; then
      child_pid="$(cat "${child_pid_file}" || true)"
    fi
    if [[ "${probe_rc}" -ne 124 ]]; then
      reap_owned_probe_child "${child_pid}"
      die "synthetic child probe expected timeout rc 124 (got ${probe_rc})"
    fi
    [[ -n "${child_pid}" && "${child_pid}" =~ ^[0-9]+$ ]] \
      || { reap_owned_probe_child "${child_pid}"; die "synthetic child pid malformed: ${child_pid}"; }
    sleep 0.2
    set +e
    kill -0 "${child_pid}" 2>/dev/null
    alive_rc=$?
    set -e
    if [[ "${alive_rc}" -eq 0 ]]; then
      reap_owned_probe_child "${child_pid}"
      die "synthetic child ${child_pid} still alive after process-group timeout (descendant cleanup failed)"
    fi
    ok "process-group-kills-synthetic-child"
  }

  # Parent exits on TERM; child ignores TERM — proves KILL escalation is not skipped on leader exit.
  {
    local probe_dir probe_sh child_pid_file alive_rc probe_rc child_pid
    probe_dir="$(mktemp -d "${TMPDIR:-/tmp}/2802-pg-termign.XXXXXX")"
    register_junction_temp "${probe_dir}"
    child_pid_file="${probe_dir}/child.pid"
    probe_sh="${probe_dir}/parent.sh"
    cat >"${probe_sh}" <<'PROBE'
#!/usr/bin/env bash
set -euo pipefail
pidfile="${1:?}"
# Child ignores SIGTERM and stays in the session; parent exits on TERM (default).
python3 - "${pidfile}" <<'CHILD' &
import os
import signal
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
Path = __import__("pathlib").Path
Path(sys.argv[1]).write_text(str(os.getpid()), encoding="utf-8")
time.sleep(120)
CHILD
# Parent waits; on group SIGTERM parent dies while child survives unless KILL follows.
wait
PROBE
    chmod +x "${probe_sh}"
    set +e
    python3 "${pg_runner}" 1 "${probe_dir}" '{"ASSAY_PG_TERM_GRACE_S":"0.4","ASSAY_PG_KILL_GRACE_S":"0.4"}' bash "${probe_sh}" "${child_pid_file}" >/dev/null 2>&1
    probe_rc=$?
    set -e
    child_pid=""
    if [[ -f "${child_pid_file}" ]]; then
      child_pid="$(cat "${child_pid_file}" || true)"
    fi
    if [[ "${probe_rc}" -ne 124 ]]; then
      reap_owned_probe_child "${child_pid}"
      die "TERM-ignore child probe expected timeout rc 124 (got ${probe_rc})"
    fi
    [[ -n "${child_pid}" && "${child_pid}" =~ ^[0-9]+$ ]] \
      || { reap_owned_probe_child "${child_pid}"; die "TERM-ignore child pid malformed: ${child_pid}"; }
    sleep 0.2
    set +e
    kill -0 "${child_pid}" 2>/dev/null
    alive_rc=$?
    set -e
    if [[ "${alive_rc}" -eq 0 ]]; then
      reap_owned_probe_child "${child_pid}"
      die "TERM-ignore child ${child_pid} still alive (KILL escalation skipped after parent exit)"
    fi
    ok "process-group-kills-term-ignoring-child"
  }

  # Eager cleanup of discover scratches; EXIT trap remains as backstop.
  cleanup_junction_temps
  ok "fixture-discover-default-three-level"
  ok "fixture-discover-explicit-glob-bounded"
  ok "docs-globstar-and-dry-run-qualifiers"

  python3 - "${ROOT}/.pre-commit-config.yaml" <<'PY2'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
block = None
lines = text.splitlines()
for i, line in enumerate(lines):
    if line.strip() == "- id: assay-action-consumer-pin":
        for j in range(i, min(i + 12, len(lines))):
            s = lines[j].strip()
            if s.startswith("files:"):
                block = s.split("files:", 1)[1].strip()
                break
        break
if not block:
    raise SystemExit("assay-action-consumer-pin files: missing")
rx = re.compile(block)
for path in (
    "scripts/ci/produce-default-discovery-sandbox-evidence.sh",
    "scripts/ci/test-action-discovery-junction.sh",
):
    if not rx.search(path):
        raise SystemExit(f"hook filter does not match {path}: {block}")
print("ok    pre-commit-pin-filter-matches-producer-and-junction")
PY2
}

if [[ "${1:-}" == "--contract-only" ]]; then
  check_explicit_glob_and_pin_filter
  echo "action discovery junction contract: PASS"
  exit 0
fi


# --- Retained mutations (must RED); all under mktemp — caller bytes untouched ---
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/2778-junction.XXXXXX")"
cleanup_scratch() { cleanup_junction_temps; rm -rf "${SCRATCH}"; }
trap cleanup_scratch EXIT

CALLER_WF="${ROOT}/.github/workflows/action-v2-test.yml"
CALLER_PROD="${ROOT}/scripts/ci/produce-default-discovery-sandbox-evidence.sh"
CALLER_WF_HASH="$(sha256sum "${CALLER_WF}" | awk '{print $1}')"
CALLER_PROD_HASH="$(sha256sum "${CALLER_PROD}" | awk '{print $1}')"

expect_red() {
  local name="$1"
  if ASSAY_JUNCTION_WORKFLOW="${SCRATCH}/wf.yml" \
     ASSAY_JUNCTION_PRODUCER="${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh" \
     bash "$0" --contract-only >"${SCRATCH}/${name}.out" 2>&1; then
    die "mutation ${name} stayed green (see ${SCRATCH}/${name}.out)"
  fi
  ok "mutation-${name}-red"
}

# Baseline copies for each mutation
seed_scratch() {
  # Mini repo layout so owned producer ROOT=.../repo still finds fixtures.
  rm -rf "${SCRATCH}/repo"
  mkdir -p "${SCRATCH}/repo/scripts/ci/fixtures/assay-action-pin"
  cp "${CALLER_WF}" "${SCRATCH}/wf.yml"
  cp "${CALLER_PROD}" "${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh"
  cp "${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd"     "${SCRATCH}/repo/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd"
  chmod +x "${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh"
}

seed_scratch
# control: scratch copies must PASS
if ! ASSAY_JUNCTION_WORKFLOW="${SCRATCH}/wf.yml" \
     ASSAY_JUNCTION_PRODUCER="${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh" \
     bash "$0" --contract-only >"${SCRATCH}/control.out" 2>&1; then
  cat "${SCRATCH}/control.out" >&2
  die "scratch control stayed red"
fi
ok "scratch-control-pass"

# 1) job-level if: false
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "  default-discovery-sandbox-junction:\n"
if needle not in text:
    raise SystemExit("junction job needle missing")
wf.write_text(text.replace(needle, needle + "    if: ${{ false }}\n", 1), encoding="utf-8")
PY
expect_red "job-if-false"

# 1b) job-level continue-on-error: true (must refuse; step-level COE rules unchanged)
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "  default-discovery-sandbox-junction:\n"
if needle not in text:
    raise SystemExit("junction job needle missing")
wf.write_text(text.replace(needle, needle + "    continue-on-error: true\n", 1), encoding="utf-8")
PY
expect_red "job-continue-on-error-true"

# 2) producer step if: false
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "      - name: Produce evidence with published remediation recipe\n"
if needle not in text:
    raise SystemExit("producer needle missing")
wf.write_text(text.replace(needle, needle + "        if: ${{ false }}\n", 1), encoding="utf-8")
PY
expect_red "producer-if-false"

# 3) assertion step if: false
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "      - name: Assert verified discovery and nonempty index digest\n"
if needle not in text:
    raise SystemExit("assertion needle missing")
wf.write_text(text.replace(needle, needle + "        if: ${{ false }}\n", 1), encoding="utf-8")
PY
expect_red "assertion-if-false"

# 4) comment out active recipe invoke in owned producer (substring remains)
seed_scratch
python3 - "${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
text = p.read_text(encoding="utf-8")
old = 'bash -c "$(cat "${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd")"'
if old not in text:
    raise SystemExit("recipe invoke needle missing in producer")
p.write_text(text.replace(old, '# ' + old, 1), encoding="utf-8")
PY
expect_red "producer-recipe-commented"

# 5) continue-on-error as string "true" on junction review
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "      - name: Review with required default discovery\n        id: review\n"
if needle not in text:
    raise SystemExit("review needle missing")
wf.write_text(
    text.replace(needle, needle + '        continue-on-error: "true"\n', 1),
    encoding="utf-8",
)
PY
expect_red "review-continue-on-error-string"

# 6) early exit 0 after set -euo in owned producer
seed_scratch
python3 - "${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
text = p.read_text(encoding="utf-8")
needle = "set -euo pipefail\n"
if needle not in text:
    raise SystemExit("set -euo needle missing")
p.write_text(text.replace(needle, needle + "exit 0\n", 1), encoding="utf-8")
PY
expect_red "producer-early-exit-0"

# 7) non-executed junction assertion: exit 0 before real checks
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "          set -euo pipefail\n          test \"${EVIDENCE_STATE}\" = \"verified\"\n"
if needle not in text:
    raise SystemExit("assertion set -euo needle missing")
wf.write_text(
    text.replace(
        needle,
        "          set -euo pipefail\n          exit 0\n          test \"${EVIDENCE_STATE}\" = \"verified\"\n",
        1,
    ),
    encoding="utf-8",
)
PY
expect_red "assertion-early-exit-0"

# 8) required-absent assertion early exit 0 (regex would still see failure compare)
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "      - name: Assert required mode fails without bundles\n        shell: bash\n        env:\n          STEP_OUTCOME: ${{ steps.review.outcome }}\n          EVIDENCE_STATE: ${{ steps.review.outputs.evidence_state }}\n        run: |\n          set -euo pipefail\n"
if needle not in text:
    raise SystemExit("required-absent assertion needle missing")
wf.write_text(
    text.replace(
        needle,
        needle.replace(
            "          set -euo pipefail\n",
            "          set -euo pipefail\n          exit 0\n",
            1,
        ),
        1,
    ),
    encoding="utf-8",
)
PY
expect_red "required-absent-assertion-early-exit-0"

# 9) corrupt assertion early exit 0
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
needle = "      - name: Assert corrupted bundle is refused\n        shell: bash\n        env:\n          STEP_OUTCOME: ${{ steps.review.outcome }}\n          EVIDENCE_STATE: ${{ steps.review.outputs.evidence_state }}\n          VERIFIED: ${{ steps.review.outputs.verified }}\n        run: |\n          set -euo pipefail\n"
if needle not in text:
    raise SystemExit("corrupt assertion needle missing")
wf.write_text(
    text.replace(
        needle,
        needle.replace(
            "          set -euo pipefail\n",
            "          set -euo pipefail\n          exit 0\n",
            1,
        ),
        1,
    ),
    encoding="utf-8",
)
PY
expect_red "corrupt-assertion-early-exit-0"

# 10) negative assertion: words only, no real outcome env binding
seed_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
wf = Path(sys.argv[1])
text = wf.read_text(encoding="utf-8")
# Weaken required-no-bundles-fails assertion: drop env binding, keep words
old = """      - name: Assert required mode fails without bundles
        shell: bash
        env:
          STEP_OUTCOME: ${{ steps.review.outcome }}
          EVIDENCE_STATE: ${{ steps.review.outputs.evidence_state }}
        run: |
          set -euo pipefail
          if [ "${STEP_OUTCOME}" != "failure" ]; then
            echo "ERROR: expected required mode with zero bundles to fail"
            exit 1
          fi
          if [ "${EVIDENCE_STATE}" = "verified" ]; then
            echo "ERROR: zero bundles must not report verified"
            exit 1
          fi
"""
new = """      - name: Assert required mode fails without bundles
        shell: bash
        run: |
          set -euo pipefail
          echo "checking outcome failure mode"
          true
"""
if old not in text:
    raise SystemExit("required-no-bundles assertion block missing")
wf.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
expect_red "weak-outcome-words-only"

# --- Fixture discovery execution mutations (M1/M5 + doc/path deletion) ---
seed_fixture_scratch() {
  rm -rf "${SCRATCH}/fixture"
  mkdir -p "${SCRATCH}/fixture"
  cp "${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml" "${SCRATCH}/fixture/action.yml"
  cp "${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE" "${SCRATCH}/fixture/PROVENANCE"
  cp "${DOC}" "${SCRATCH}/fixture/github-action.md"
  cp "${CALLER_WF}" "${SCRATCH}/wf.yml"
  mkdir -p "${SCRATCH}/repo/scripts/ci"
  cp "${CALLER_PROD}" "${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh"
  chmod +x "${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh"
}

coord_fixture_sha() {
  local dig
  dig="$(sha256sum "${SCRATCH}/fixture/action.yml" | awk '{print $1}')"
  python3 - "${SCRATCH}/fixture/PROVENANCE" "${dig}" <<'PYSHA'
from pathlib import Path
import sys
path, dig = Path(sys.argv[1]), sys.argv[2]
lines = []
for line in path.read_text().splitlines(True):
    if line.startswith("sha256="):
        lines.append(f"sha256={dig}\n")
    else:
        lines.append(line)
path.write_text("".join(lines))
PYSHA
}

expect_red_fixture() {
  local name="$1"
  if ASSAY_JUNCTION_WORKFLOW="${SCRATCH}/wf.yml" \
     ASSAY_JUNCTION_PRODUCER="${SCRATCH}/repo/scripts/ci/produce-default-discovery-sandbox-evidence.sh" \
     ASSAY_JUNCTION_ACTION_YML="${SCRATCH}/fixture/action.yml" \
     ASSAY_JUNCTION_DOC="${SCRATCH}/fixture/github-action.md" \
     bash "$0" --contract-only >"${SCRATCH}/${name}.out" 2>&1; then
    die "mutation ${name} stayed green (see ${SCRATCH}/${name}.out)"
  fi
  ok "mutation-${name}-red"
}

# M1: fixture gains shopt -s globstar (+ coordinated PROVENANCE)
seed_fixture_scratch
python3 - "${SCRATCH}/fixture/action.yml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = "          # Glob expansion: use compgen for safe pattern matching\n"
insert = needle + "          shopt -s globstar\n"
if needle not in text:
    raise SystemExit("M1 needle missing")
path.write_text(text.replace(needle, insert, 1), encoding="utf-8")
PY
coord_fixture_sha
expect_red_fixture "fixture-globstar-enabled"

# M5: fixture find gains -maxdepth 3 (+ coordinated PROVENANCE)
seed_fixture_scratch
python3 - "${SCRATCH}/fixture/action.yml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "find . \\( -path './.assay/evidence/*.tar.gz' -o -path './evidence/*.tar.gz' \\) -type f"
new = "find . -maxdepth 3 \\( -path './.assay/evidence/*.tar.gz' -o -path './evidence/*.tar.gz' \\) -type f"
if old not in text:
    raise SystemExit("M5 find needle missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
coord_fixture_sha
expect_red_fixture "fixture-find-maxdepth"

# Removal of effective default find command must bite
seed_fixture_scratch
python3 - "${SCRATCH}/fixture/action.yml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "          find . \\( -path './.assay/evidence/*.tar.gz' -o -path './evidence/*.tar.gz' \\) -type f 2>/dev/null >> \"$RAW\"\n"
new = "          # find removed by mutation\n          true\n"
if old not in text:
    raise SystemExit("find removal needle missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
coord_fixture_sha
expect_red_fixture "fixture-find-removed"

# Docs: delete globstar qualifier paragraph
seed_fixture_scratch
python3 - "${SCRATCH}/fixture/github-action.md" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = (
    "An explicit `bundles:` value is expanded with Bash `compgen -G` and does **not**\n"
    "enable `globstar`. A pattern like `.assay/evidence/**/*.tar.gz` therefore does\n"
    "**not** mean arbitrary-depth recursion: without `globstar`, `**` selects about\n"
    "one intermediate directory level (for example `.assay/evidence/mid/*.tar.gz`),\n"
    "misses top-level `.assay/evidence/*.tar.gz`, and misses deeper trees such as\n"
    "`.assay/evidence/mid/deep/*.tar.gz`. Do not copy a `**` example expecting full\n"
    "recursion; omit `bundles` instead.\n"
)
if old not in text:
    raise SystemExit("globstar doc paragraph missing")
path.write_text(text.replace(old, "", 1), encoding="utf-8")
PY
expect_red_fixture "docs-globstar-paragraph-deleted"

# Docs: delete observed-effects dry-run qualification
seed_fixture_scratch
python3 - "${SCRATCH}/fixture/github-action.md" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = (
    "\nThat recipe attests the sandbox command's observed effects, not that a test suite\n"
    "passed.\n"
)
if old not in text:
    raise SystemExit("dry-run qualification missing")
path.write_text(text.replace(old, "\n", 1), encoding="utf-8")
PY
expect_red_fixture "docs-dry-run-qualifier-deleted"

# Workflow paths: drop producer entry
seed_fixture_scratch
python3 - "${SCRATCH}/wf.yml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "      - 'scripts/ci/produce-default-discovery-sandbox-evidence.sh'\n"
if old not in text:
    raise SystemExit("producer path entry missing")
path.write_text(text.replace(old, "", 1), encoding="utf-8")
PY
expect_red_fixture "workflow-producer-path-deleted"

# Caller bytes must be unchanged even after mutations / interruption path
AFTER_WF_HASH="$(sha256sum "${CALLER_WF}" | awk '{print $1}')"
AFTER_PROD_HASH="$(sha256sum "${CALLER_PROD}" | awk '{print $1}')"
[[ "${AFTER_WF_HASH}" == "${CALLER_WF_HASH}" ]] || die "caller workflow bytes changed during mutations"
[[ "${AFTER_PROD_HASH}" == "${CALLER_PROD_HASH}" ]] || die "caller producer bytes changed during mutations"
ok "caller-bytes-unchanged"

trap - EXIT
cleanup_scratch

check_explicit_glob_and_pin_filter
echo "action discovery junction contract: PASS"
