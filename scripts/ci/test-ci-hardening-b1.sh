#!/usr/bin/env bash
# Wave B1 contract: single-source Assay release-tag pin, Structurizr image pin,
# timeouts on touched jobs, and a version-independent CI infra doc header.
#
# Expected release tag is derived from Cargo.toml [workspace.package].version
# (independent of the production pin reader). Digests live only in the
# Structurizr pin file — never as test or consumer literals.
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
STRUCTURIZR_IMAGE_PIN="${ROOT}/.github/structurizr-cli-image"
CI_DOC="${ROOT}/docs/AIcontext/ci-infrastructure.md"
THIS_TEST="${ROOT}/scripts/ci/test-ci-hardening-b1.sh"

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
scratch="$(mktemp -d)"
trap 'abort_is_failure "$?"' ERR
trap 'rm -rf "${scratch}"' EXIT

# Independent of the production pin reader: the release line is v + workspace version.
workspace_version="$(
  awk '
    $0 == "[workspace.package]" { in_workspace_package = 1; next }
    /^\[/ && $0 != "[workspace.package]" { in_workspace_package = 0 }
    in_workspace_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "${ROOT}/Cargo.toml"
)"
if [[ ! "${workspace_version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  fail "could not read [workspace.package].version from Cargo.toml (got '${workspace_version}')"
  echo "ci-hardening-b1 contract: ${failures} failure(s)" >&2
  exit 1
fi
EXPECTED_TAG="v${workspace_version}"

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

is_digest_image_ref() {
  local ref="$1"
  [[ "${ref}" =~ ^[A-Za-z0-9._/-]+@sha256:[0-9a-f]{64}$ ]]
}

echo "== pin file matches workspace release line ${EXPECTED_TAG} =="
if [[ -f "${PIN_FILE}" ]]; then
  pin_raw="$(tr -d '[:space:]' <"${PIN_FILE}")"
  if [[ "${pin_raw}" == "${EXPECTED_TAG}" ]]; then
    ok "pin file is ${EXPECTED_TAG} (v + workspace.package.version)"
  else
    fail "pin file content is '${pin_raw}', expected ${EXPECTED_TAG} from Cargo.toml"
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

echo "== malformed Assay pin fails closed =="
if [[ -f "${READER}" ]]; then
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
  if grep -qE 'assay-release-tag' "${wf}" && ! grep -qE 'read-assay-release-tag\.sh' "${wf}"; then
    fail "${name} references the pin file without the shared reader"
  fi
done

if grep -qE 'version:\s*\$\{\{\s*steps\.[^}]+\.outputs\.version\s*\}\}' "${ASSAY_WF}"; then
  ok "assay.yml composite input uses steps.*.outputs.version"
else
  fail "assay.yml does not bind version: \${{ steps.*.outputs.version }}"
fi

if grep -qE 'ASSAY_VERSION:\s*\$\{\{\s*steps\.[^}]+\.outputs\.version\s*\}\}' "${SECURITY_WF}"; then
  ok "assay-security.yml ASSAY_VERSION uses steps.*.outputs.version"
else
  fail "assay-security.yml does not bind ASSAY_VERSION to steps.*.outputs.version"
fi

echo "== Structurizr image pin file exists and has valid digest form =="
if [[ -f "${STRUCTURIZR_IMAGE_PIN}" ]]; then
  ok "image pin file present"
else
  fail "missing ${STRUCTURIZR_IMAGE_PIN#"${ROOT}"/}"
fi

echo "== Structurizr image helper reads the pin (no embedded digest) =="
if [[ -f "${STRUCTURIZR_IMAGE_HELPER}" ]]; then
  if grep -qE 'sha256:[0-9a-f]{64}' "${STRUCTURIZR_IMAGE_HELPER}"; then
    fail "structurizr-cli-image.sh embeds a digest literal (pin file must be the only source)"
  else
    ok "structurizr-cli-image.sh has no digest literal"
  fi
  if [[ -f "${STRUCTURIZR_IMAGE_PIN}" ]]; then
    image="$("${STRUCTURIZR_IMAGE_HELPER}" 2>/dev/null || true)"
    pin_raw="$(
      lines=()
      while IFS= read -r line || [[ -n "${line}" ]]; do
        line="${line//$'\r'/}"
        line="${line#"${line%%[![:space:]]*}"}"
        line="${line%"${line##*[![:space:]]}"}"
        [[ -z "${line}" ]] && continue
        lines+=("${line}")
      done <"${STRUCTURIZR_IMAGE_PIN}"
      if [[ "${#lines[@]}" -eq 1 ]]; then
        printf '%s\n' "${lines[0]}"
      fi
    )"
    if is_digest_image_ref "${image}" && [[ "${image}" == "${pin_raw}" ]]; then
      ok "helper returns the pin file image reference"
    else
      fail "helper returned '${image}', pin file is '${pin_raw}' (want identical name@sha256:<64 hex>)"
    fi
  else
    fail "cannot compare helper to missing pin file"
  fi
else
  fail "missing ${STRUCTURIZR_IMAGE_HELPER#"${ROOT}"/}"
fi

echo "== malformed Structurizr image pin fails closed =="
if [[ -f "${STRUCTURIZR_IMAGE_HELPER}" ]]; then
  bad_image="${scratch}/structurizr-cli-image"
  printf 'structurizr/cli:latest\n' >"${bad_image}"
  if STRUCTURIZR_CLI_IMAGE_FILE="${bad_image}" "${STRUCTURIZR_IMAGE_HELPER}" >/dev/null 2>"${scratch}/img-err"; then
    fail "image helper accepted ':latest' pin"
  else
    ok "image helper rejects :latest"
  fi
  printf 'structurizr/cli@sha256:deadbeef\n' >"${bad_image}"
  if STRUCTURIZR_CLI_IMAGE_FILE="${bad_image}" "${STRUCTURIZR_IMAGE_HELPER}" >/dev/null 2>"${scratch}/img-err"; then
    fail "image helper accepted short digest"
  else
    ok "image helper rejects short digest"
  fi
  : >"${bad_image}"
  if STRUCTURIZR_CLI_IMAGE_FILE="${bad_image}" "${STRUCTURIZR_IMAGE_HELPER}" >/dev/null 2>"${scratch}/img-err"; then
    fail "image helper accepted empty pin"
  else
    ok "image helper rejects empty pin"
  fi
  # Build multi-line content at runtime so this test file never embeds a digest literal.
  {
    printf 'structurizr/cli@sha256:%s\n' "$(python3 -c 'print("a" * 64)')"
    printf 'structurizr/cli@sha256:%s\n' "$(python3 -c 'print("b" * 64)')"
  } >"${bad_image}"
  if STRUCTURIZR_CLI_IMAGE_FILE="${bad_image}" "${STRUCTURIZR_IMAGE_HELPER}" >/dev/null 2>"${scratch}/img-err"; then
    fail "image helper accepted multi-line pin"
  else
    ok "image helper rejects multi-line pin"
  fi
else
  fail "cannot prove image-pin fail-closed; helper missing"
fi

echo "== Structurizr consumers use the helper only (no :latest, no digest literal) =="
for path in "${STRUCTURIZR_WF}" "${STRUCTURIZR_VALIDATE}" "${STRUCTURIZR_EXPORT}"; do
  name="${path#"${ROOT}"/}"
  if grep -qE 'structurizr/cli:latest' "${path}"; then
    fail "${name} still references structurizr/cli:latest"
  else
    ok "${name} has no :latest"
  fi
  if grep -qE 'sha256:[0-9a-f]{64}' "${path}"; then
    fail "${name} embeds a digest literal (second source)"
  else
    ok "${name} has no digest literal"
  fi
done

for path in "${STRUCTURIZR_VALIDATE}" "${STRUCTURIZR_EXPORT}"; do
  name="${path#"${ROOT}"/}"
  if grep -qE 'structurizr-cli-image\.sh' "${path}"; then
    ok "${name} uses structurizr-cli-image.sh"
  else
    fail "${name} does not call scripts/structurizr-cli-image.sh"
  fi
done

if python3 - "${STRUCTURIZR_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
jobs = text.split("\njobs:", 1)
if len(jobs) != 2:
    sys.exit(1)
body = jobs[1]
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

if grep -qE '^\s*-\s*"\.github/structurizr-cli-image"\s*$' "${STRUCTURIZR_WF}"; then
  ok "structurizr-validate.yml paths filter includes the image pin file"
else
  fail "structurizr-validate.yml paths filter missing .github/structurizr-cli-image"
fi

echo "== contract test carries no digest literal =="
if grep -qE 'sha256:[0-9a-f]{64}' "${THIS_TEST}"; then
  fail "test-ci-hardening-b1.sh embeds a digest literal (violates single-source)"
else
  ok "contract test has no digest literal"
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
