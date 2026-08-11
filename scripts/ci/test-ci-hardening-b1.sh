#!/usr/bin/env bash
# Wave B1 contract: single-source Assay release-tag pin, Structurizr digest pin,
# timeouts on touched jobs, and a version-independent CI infra doc header.
#
# Failures here are the intended RED state before production edits land.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIN_FILE="${ROOT}/.github/assay-release-tag"
READER="${ROOT}/scripts/ci/read-assay-release-tag.sh"
ASSAY_WF="${ROOT}/.github/workflows/assay.yml"
SECURITY_WF="${ROOT}/.github/workflows/assay-security.yml"
STRUCTURIZR_WF="${ROOT}/.github/workflows/structurizr-validate.yml"
STRUCTURIZR_VALIDATE="${ROOT}/scripts/structurizr-validate.sh"
STRUCTURIZR_EXPORT="${ROOT}/scripts/structurizr-export.sh"
STRUCTURIZR_IMAGE_HELPER="${ROOT}/scripts/structurizr-cli-image.sh"
CI_DOC="${ROOT}/docs/AIcontext/ci-infrastructure.md"
EXPECTED_DIGEST="sha256:717e320e0ad52335ea9939bf5fae092620cc3deccecf6f280a5b6fee99763c53"
EXPECTED_TAG="v5.1.0"

failures=0

fail() {
  echo "FAIL: $*" >&2
  failures=$((failures + 1))
}

ok() {
  echo "ok   $*"
}

abort_is_failure() {
  local rc="$1"
  if [[ "${rc}" -ne 0 ]]; then
    echo "ci-hardening-b1 contract aborted (exit ${rc}); treat as failure" >&2
  fi
}
trap 'abort_is_failure "$?"' ERR

job_has_timeout() {
  local wf="$1" job="$2"
  python3 - "$wf" "$job" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
job = sys.argv[2]
match = re.search(rf"^  {re.escape(job)}:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text, re.S | re.M)
if not match:
    sys.exit(1)
block = match.group(1)
sys.exit(0 if re.search(r"(?m)^    timeout-minutes:\s*[1-9][0-9]*\s*$", block) else 1)
PY
}

echo "== pin file exists and names ${EXPECTED_TAG} =="
if [[ -f "${PIN_FILE}" ]]; then
  pin_raw="$(tr -d '[:space:]' <"${PIN_FILE}")"
  if [[ "${pin_raw}" == "${EXPECTED_TAG}" ]]; then
    ok "pin file is ${EXPECTED_TAG}"
  else
    fail "pin file content is '${pin_raw}', expected ${EXPECTED_TAG}"
  fi
else
  fail "missing ${PIN_FILE#"${ROOT}"/}"
fi

echo "== single reader helper exists =="
if [[ -x "${READER}" ]] || [[ -f "${READER}" ]]; then
  ok "reader script present"
else
  fail "missing ${READER#"${ROOT}"/}"
fi

echo "== reader accepts the checked-in pin =="
if [[ -f "${READER}" ]]; then
  got="$("${READER}" 2>/dev/null || true)"
  if [[ "${got}" == "${EXPECTED_TAG}" ]]; then
    ok "reader returns ${EXPECTED_TAG}"
  else
    fail "reader returned '${got}', expected ${EXPECTED_TAG}"
  fi
else
  fail "cannot exercise reader; script missing"
fi

echo "== malformed pin fails closed =="
if [[ -f "${READER}" ]]; then
  scratch="$(mktemp -d)"
  trap 'rm -rf "${scratch}"' EXIT
  bad_pin="${scratch}/assay-release-tag"
  printf 'not-a-tag\n' >"${bad_pin}"
  if ASSAY_RELEASE_TAG_FILE="${bad_pin}" "${READER}" >/dev/null 2>"${scratch}/err"; then
    fail "reader accepted malformed pin 'not-a-tag'"
  else
    ok "reader rejects malformed pin"
  fi
  printf 'v1.2\n' >"${bad_pin}"
  if ASSAY_RELEASE_TAG_FILE="${bad_pin}" "${READER}" >/dev/null 2>"${scratch}/err"; then
    fail "reader accepted incomplete semver 'v1.2'"
  else
    ok "reader rejects incomplete semver"
  fi
  : >"${bad_pin}"
  if ASSAY_RELEASE_TAG_FILE="${bad_pin}" "${READER}" >/dev/null 2>"${scratch}/err"; then
    fail "reader accepted empty pin"
  else
    ok "reader rejects empty pin"
  fi
  printf 'v5.1.0\nv9.9.9\n' >"${bad_pin}"
  if ASSAY_RELEASE_TAG_FILE="${bad_pin}" "${READER}" >/dev/null 2>"${scratch}/err"; then
    fail "reader accepted multi-line pin"
  else
    ok "reader rejects multi-line pin"
  fi
else
  fail "cannot prove fail-closed; reader missing"
fi

echo "== both workflows consume the reader output (no second literal/parser) =="
for wf in "${ASSAY_WF}" "${SECURITY_WF}"; do
  name="${wf#"${ROOT}"/}"
  if grep -qE 'read-assay-release-tag\.sh' "${wf}"; then
    ok "${name} invokes the reader"
  else
    fail "${name} does not invoke scripts/ci/read-assay-release-tag.sh"
  fi
  if grep -qE 'v2\.12\.0|version:\s*v[0-9]|ASSAY_VERSION:\s*v[0-9]' "${wf}"; then
    fail "${name} still embeds a release-tag literal (second source)"
  else
    ok "${name} has no embedded release-tag literal"
  fi
  # A second parser would be sed/awk/cat of the pin file inline in the workflow.
  if grep -qE 'assay-release-tag' "${wf}" && ! grep -qE 'read-assay-release-tag\.sh' "${wf}"; then
    fail "${name} references the pin file without the shared reader"
  fi
done

# assay.yml must wire the composite input from the step output.
if grep -qE 'version:\s*\$\{\{\s*steps\.[^}]+\.outputs\.version\s*\}\}' "${ASSAY_WF}"; then
  ok "assay.yml composite input uses steps.*.outputs.version"
else
  fail "assay.yml does not bind version: \${{ steps.*.outputs.version }}"
fi

# assay-security.yml must wire ASSAY_VERSION from the same output shape.
if grep -qE 'ASSAY_VERSION:\s*\$\{\{\s*steps\.[^}]+\.outputs\.version\s*\}\}' "${SECURITY_WF}"; then
  ok "assay-security.yml ASSAY_VERSION uses steps.*.outputs.version"
else
  fail "assay-security.yml does not bind ASSAY_VERSION to steps.*.outputs.version"
fi

echo "== Structurizr image is digest-pinned (no :latest) =="
if [[ -f "${STRUCTURIZR_IMAGE_HELPER}" ]]; then
  image="$("${STRUCTURIZR_IMAGE_HELPER}" 2>/dev/null || true)"
  if [[ "${image}" == "structurizr/cli@${EXPECTED_DIGEST}" ]]; then
    ok "image helper returns exact digest pin"
  else
    fail "image helper returned '${image}', expected structurizr/cli@${EXPECTED_DIGEST}"
  fi
else
  fail "missing ${STRUCTURIZR_IMAGE_HELPER#"${ROOT}"/}"
fi

for path in "${STRUCTURIZR_WF}" "${STRUCTURIZR_VALIDATE}" "${STRUCTURIZR_EXPORT}"; do
  name="${path#"${ROOT}"/}"
  if grep -qE 'structurizr/cli:latest' "${path}"; then
    fail "${name} still references structurizr/cli:latest"
  else
    ok "${name} has no :latest"
  fi
done

# Consumers must obtain the image from the helper, not a second literal.
for path in "${STRUCTURIZR_VALIDATE}" "${STRUCTURIZR_EXPORT}"; do
  name="${path#"${ROOT}"/}"
  if grep -qE 'structurizr-cli-image\.sh' "${path}"; then
    ok "${name} uses structurizr-cli-image.sh"
  else
    fail "${name} does not call scripts/structurizr-cli-image.sh"
  fi
  if grep -qE 'structurizr/cli@sha256:' "${path}"; then
    fail "${name} embeds a digest literal (second source)"
  else
    ok "${name} has no embedded digest literal"
  fi
done

# Workflow must call the validate script or image helper in an active step — not merely
# list the path in `on.pull_request.paths` (that matched before any production edit).
if python3 - "${STRUCTURIZR_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
jobs = text.split("\njobs:", 1)
if len(jobs) != 2:
    sys.exit(1)
body = jobs[1]
# Active step lines (not comments).
active = [
    line
    for line in body.splitlines()
    if line.strip() and not line.lstrip().startswith("#")
]
joined = "\n".join(active)
if re.search(r"structurizr-validate\.sh|structurizr-cli-image\.sh", joined):
    sys.exit(0)
sys.exit(1)
PY
then
  ok "structurizr-validate.yml job steps use shared script/helper"
else
  fail "structurizr-validate.yml job steps neither call structurizr-validate.sh nor structurizr-cli-image.sh"
fi
if grep -qE 'structurizr/cli@sha256:717e320e0ad52335ea9939bf5fae092620cc3deccecf6f280a5b6fee99763c53' \
  "${STRUCTURIZR_WF}"; then
  fail "structurizr-validate.yml embeds the digest literal (second source)"
fi

echo "== touched jobs declare timeout-minutes =="
if job_has_timeout "${ASSAY_WF}" "assay"; then
  ok "assay.yml assay job has timeout-minutes"
else
  fail "assay.yml assay job missing timeout-minutes"
fi
if job_has_timeout "${SECURITY_WF}" "security-check"; then
  ok "assay-security.yml security-check job has timeout-minutes"
else
  fail "assay-security.yml security-check job missing timeout-minutes"
fi
if job_has_timeout "${STRUCTURIZR_WF}" "validate"; then
  ok "structurizr-validate.yml validate job has timeout-minutes"
else
  fail "structurizr-validate.yml validate job missing timeout-minutes"
fi

echo "== CI infra doc header is version-independent =="
if [[ -f "${CI_DOC}" ]]; then
  header="$(sed -n '1,8p' "${CI_DOC}")"
  if grep -Eq '^\s*>\s*\*\*Version\*\*:' <<<"${header}"; then
    fail "ci-infrastructure.md header still carries a release Version line that will drift"
  else
    ok "ci-infrastructure.md header has no Version pin"
  fi
  if grep -Eq '3\.9\.0|April 2026' <<<"${header}"; then
    fail "ci-infrastructure.md header still names stale 3.9.0 / April 2026"
  else
    ok "ci-infrastructure.md header is free of stale version/date"
  fi
else
  fail "missing ${CI_DOC#"${ROOT}"/}"
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "ci-hardening-b1 contract: ${failures} failure(s)" >&2
  exit 1
fi

echo "ci-hardening-b1 contract: PASS"
