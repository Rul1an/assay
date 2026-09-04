#!/usr/bin/env bash
# #2778: pin + published nested sandbox recipe + required default-discovery journey.
# Single recipe source: scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd
# (bytes from the peeled Action commit). Workflows/docs must consume that file;
# they must not restate the command or teach fixture-copy / `assay run` remediation.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
FIXTURE="${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml"
PROVENANCE="${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE"
RECIPE="${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd"
WORKFLOW="${ROOT}/.github/workflows/action-v2-test.yml"
DOC="${ROOT}/docs/guides/github-action.md"
READER="${ROOT}/scripts/ci/read-assay-action-pin.sh"
EXPECTED_PIN="651c82109dc2200ba45e19775bf92cf68f7712ea"

die() { echo "action-discovery-junction: $*" >&2; exit 1; }
ok() { echo "ok    $*"; }

PIN="$("${READER}")"
[[ "${PIN}" == "${EXPECTED_PIN}" ]] || die "pin ${PIN} != peeled v3.2.0 ${EXPECTED_PIN}"

[[ -f "${RECIPE}" ]] || die "missing single-source recipe ${RECIPE}"
RECIPE_BODY="$(cat "${RECIPE}")"
[[ -n "${RECIPE_BODY}" ]] || die "recipe file is empty"
[[ "${RECIPE_BODY}" == *"assay sandbox"* ]] || die "recipe must invoke assay sandbox"
[[ "${RECIPE_BODY}" == *".assay/evidence/nested/"* ]] || die "recipe must write nested discovery path"
[[ "${RECIPE_BODY}" != *"assay run"* ]] || die "recipe must not teach assay run"

# Vendored action.yml must not revive stale remediation.
if grep -Fq "Run 'assay run'" "${FIXTURE}"; then
  die "fixture action.yml still teaches Run 'assay run' (stale remediation)"
fi
if grep -Fq "assay run --policy" "${FIXTURE}"; then
  die "fixture action.yml still teaches assay run --policy remediation"
fi
grep -Fq 'remediation_recipe.cmd' "${FIXTURE}" || die "fixture action.yml must load remediation_recipe.cmd"

# PROVENANCE must name the same peel + v3.2.0.
grep -Eq "^commit=${EXPECTED_PIN}$" "${PROVENANCE}" || die "PROVENANCE commit drift"
grep -Eq "^tag=v3.2.0$" "${PROVENANCE}" || die "PROVENANCE tag must be v3.2.0"

# Docs: troubleshooting must embed the exact recipe bytes (one source).
DOC_TEXT="$(cat "${DOC}")"
[[ "${DOC_TEXT}" == *"${RECIPE_BODY}"* ]] || die "docs/guides/github-action.md must embed remediation_recipe.cmd bytes exactly"
if grep -n "Generate with:" -A6 "${DOC}" | grep -Fq "assay ci"; then
  die "docs troubleshooting still generates evidence via assay ci (stale)"
fi
if grep -Fq -- "--out evidence.tar.gz" "${DOC}"; then
  die "docs still export to evidence.tar.gz outside Action discovery roots"
fi

# Workflow junction job: consume recipe file, required mode, no bundles override, no fixture cp.
WF="$(cat "${WORKFLOW}")"
echo "${WF}" | grep -Fq 'default-discovery-sandbox-junction' || die "action-v2-test.yml missing default-discovery-sandbox-junction job"
echo "${WF}" | grep -Fq 'scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd' || die "junction job must run recipe via fixture path (no duplicated literal)"
# Required mode present near the junction uses block: extract job roughly
python3 - "${WORKFLOW}" "${EXPECTED_PIN}" <<'PY'
import re, sys
from pathlib import Path
wf = Path(sys.argv[1]).read_text(encoding="utf-8")
pin = sys.argv[2]
# Split on job keys at column 0 indent of 2 spaces
m = re.search(
    r"(?m)^  default-discovery-sandbox-junction:\n(.*?)(?=\n  [A-Za-z0-9_-]+:|\Z)",
    wf,
    re.S,
)
if not m:
    raise SystemExit("junction job block not found")
job = m.group(0)
if "evidence_mode: required" not in job:
    raise SystemExit("junction job must set evidence_mode: required")
if re.search(r"(?m)^\s+bundles:\s*", job):
    raise SystemExit("junction job must omit bundles: (default discovery only)")
if "cp tests/fixtures/evidence/" in job:
    raise SystemExit("junction job must not copy fixture evidence (false-green producer)")
if f"Rul1an/assay-action@{pin}" not in job:
    raise SystemExit(f"junction job must uses: Rul1an/assay-action@{pin}")
if "evidence_state" not in job or "evidence_index_digest" not in job:
    raise SystemExit("junction job must assert evidence_state and evidence_index_digest")
if "remediation_recipe.cmd" not in job:
    raise SystemExit("junction job must reference remediation_recipe.cmd")
print("ok    junction-job-shape")
PY

ok "pin-recipe-doc-workflow-junction"
echo "action discovery junction contract: PASS"
