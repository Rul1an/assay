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

if [[ "${1:-}" == "--contract-only" ]]; then
  echo "action discovery junction contract: PASS"
  exit 0
fi

# --- Retained mutations (must RED); all under mktemp — caller bytes untouched ---
SCRATCH="$(mktemp -d "${TMPDIR:-/tmp}/2778-junction.XXXXXX")"
cleanup_scratch() { rm -rf "${SCRATCH}"; }
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

# Caller bytes must be unchanged even after mutations / interruption path
AFTER_WF_HASH="$(sha256sum "${CALLER_WF}" | awk '{print $1}')"
AFTER_PROD_HASH="$(sha256sum "${CALLER_PROD}" | awk '{print $1}')"
[[ "${AFTER_WF_HASH}" == "${CALLER_WF_HASH}" ]] || die "caller workflow bytes changed during mutations"
[[ "${AFTER_PROD_HASH}" == "${CALLER_PROD_HASH}" ]] || die "caller producer bytes changed during mutations"
ok "caller-bytes-unchanged"

trap - EXIT
cleanup_scratch

echo "action discovery junction contract: PASS"
