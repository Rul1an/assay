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
CI_WF="${ROOT}/.github/workflows/ci.yml"
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
  # Exactly the Structurizr CLI repository — any other name@digest is hostile.
  [[ "${ref}" =~ ^structurizr/cli@sha256:[0-9a-f]{64}$ ]]
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
  printf 'v1.2.3\nv9.9.9\n' >"${bad_pin}"
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

echo "== assay-security.yml push.paths cover pin inputs =="
# push.paths must name every new dependency the job reads, plus the workflow
# itself. pull_request stays unfiltered so PR coverage is unchanged.
if python3 - "${SECURITY_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
# pull_request must remain present and not gain a paths: filter under on:.
on_match = re.search(r"(?ms)^on:\n(.*?)(?=^permissions:|^jobs:)", text)
if not on_match:
    sys.exit("missing on:")
on_block = on_match.group(1)
if not re.search(r"(?m)^  pull_request:\s*$", on_block):
    sys.exit("pull_request trigger missing or altered")
# No paths: indented under pull_request (unfiltered).
pr_section = re.search(
    r"(?ms)^  pull_request:\n((?:    .+\n)*)",
    on_block,
)
if pr_section and re.search(r"(?m)^    paths:", pr_section.group(1)):
    sys.exit("pull_request gained a paths filter")

push = re.search(r"(?ms)^  push:\n((?:    .+\n)*)", on_block)
if not push:
    sys.exit("push trigger missing")
paths = re.findall(r'(?m)^      - "([^"]+)"\s*$', push.group(1))
required = {
    ".github/assay-release-tag",
    "scripts/ci/read-assay-release-tag.sh",
    ".github/workflows/assay-security.yml",
}
missing = sorted(required - set(paths))
if missing:
    sys.exit("push.paths missing: " + ", ".join(missing))
sys.exit(0)
PY
then
  ok "assay-security.yml push.paths include pin, reader, and workflow; pull_request unfiltered"
else
  fail "assay-security.yml push.paths incomplete or pull_request behavior changed"
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
      fail "helper returned '${image}', pin file is '${pin_raw}' (want identical structurizr/cli@sha256:<64 hex>)"
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
  # Foreign repository with a valid-looking digest must not be accepted.
  printf 'evil.example/cli@sha256:%s\n' "$(python3 -c 'print("c" * 64)')" >"${bad_image}"
  if STRUCTURIZR_CLI_IMAGE_FILE="${bad_image}" "${STRUCTURIZR_IMAGE_HELPER}" >/dev/null 2>"${scratch}/img-err"; then
    fail "image helper accepted foreign repository evil.example/cli@sha256:<64 hex>"
  else
    ok "image helper rejects foreign repository (evil.example/cli)"
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

echo "== CI forces pinned Docker route (no native CLI bypass) =="
# The workflow must set STRUCTURIZR_FORCE_DOCKER on the validate step so a
# coincidentally installed structurizr-cli cannot skip the digest-pinned image.
if python3 - "${STRUCTURIZR_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
jobs = text.split("\njobs:", 1)
if len(jobs) != 2:
    sys.exit("no jobs")
body = jobs[1]
# Find the Validate Structurizr workspaces step and require FORCE_DOCKER in its env/run.
step_pat = re.compile(
    r"- name: Validate Structurizr workspaces\n(?P<body>(?:[ \t]+.+\n)+)",
    re.M,
)
m = step_pat.search(body)
if not m:
    sys.exit("validate step missing")
step = m.group("body")
if "STRUCTURIZR_FORCE_DOCKER" not in step:
    sys.exit("FORCE_DOCKER unset")
# Must be an active assignment to a truthy value, not a comment.
active = [
    line
    for line in step.splitlines()
    if line.strip() and not line.lstrip().startswith("#")
]
joined = "\n".join(active)
if not re.search(
    r"STRUCTURIZR_FORCE_DOCKER:\s*['\"]?[1ty]",
    joined,
    re.I,
) and "STRUCTURIZR_FORCE_DOCKER=1" not in joined:
    sys.exit("FORCE_DOCKER not truthy")
if "structurizr-validate.sh" not in joined:
    sys.exit("does not call validate script")
sys.exit(0)
PY
then
  ok "structurizr-validate.yml forces STRUCTURIZR_FORCE_DOCKER for pinned Docker"
else
  fail "structurizr-validate.yml does not force STRUCTURIZR_FORCE_DOCKER on the validate step"
fi

# Behavioral: force-docker must not fall back to native CLI when Docker is unusable.
# ubuntu-latest ships /usr/bin/docker; omitting a stub lets the real binary succeed and
# falsely green this case. Put an unusable docker stub first (exit 127) so discovery
# finds docker but every invocation fails.
force_bin="${scratch}/force-bin"
mkdir -p "${force_bin}"
cat >"${force_bin}/structurizr-cli" <<'STUB'
#!/usr/bin/env bash
echo "native-cli-invoked" >>"${STRUCTURIZR_STUB_LOG}"
exit 0
STUB
chmod +x "${force_bin}/structurizr-cli"
cat >"${force_bin}/docker" <<'STUB'
#!/usr/bin/env bash
echo "docker-unusable-invoked:$*" >>"${STRUCTURIZR_STUB_LOG}"
exit 127
STUB
chmod +x "${force_bin}/docker"
: >"${scratch}/force.log"
force_rc=0
STRUCTURIZR_FORCE_DOCKER=1 STRUCTURIZR_STUB_LOG="${scratch}/force.log" \
  PATH="${force_bin}:/usr/bin:/bin" \
  bash "${STRUCTURIZR_VALIDATE}" >"${scratch}/force.out" 2>&1 || force_rc=$?
if [[ "${force_rc}" -eq 0 ]]; then
  fail "STRUCTURIZR_FORCE_DOCKER=1 succeeded with unusable docker (native bypass or empty pass)"
elif grep -q 'native-cli-invoked' "${scratch}/force.log"; then
  fail "STRUCTURIZR_FORCE_DOCKER=1 fell back to native structurizr-cli"
elif ! grep -q 'docker-unusable-invoked:' "${scratch}/force.log"; then
  fail "STRUCTURIZR_FORCE_DOCKER=1 did not invoke the unusable docker stub"
else
  ok "STRUCTURIZR_FORCE_DOCKER=1 fails closed with unusable docker (no native fallback)"
fi

# Behavioral: overwrite with a successful docker stub; force-docker must use it, not native.
cat >"${force_bin}/docker" <<'STUB'
#!/usr/bin/env bash
echo "docker-invoked:$*" >>"${STRUCTURIZR_STUB_LOG}"
exit 0
STUB
chmod +x "${force_bin}/docker"
: >"${scratch}/force-docker.log"
# Use an empty workspace glob sandbox by pointing at a temp tree with no DSL —
# still exercises the route selection before/at the loop. Prefer a fixture dir
# that has at least one workspace so docker is actually invoked.
fixture_root="${scratch}/sz-root"
mkdir -p "${fixture_root}/docs/architecture/structurizr/demo"
printf 'workspace "demo" {}\n' >"${fixture_root}/docs/architecture/structurizr/demo/workspace.dsl"
# Re-run validate from a copy that resolves ROOT relative to the script location;
# the real script uses its own ROOT. Instead invoke with a wrapper that cds and
# relies on the real script — create a temporary validate that uses our stubs
# by running the production script under PATH stubs; workspaces come from repo.
STRUCTURIZR_FORCE_DOCKER=1 STRUCTURIZR_STUB_LOG="${scratch}/force-docker.log" \
  PATH="${force_bin}:/usr/bin:/bin" \
  bash "${STRUCTURIZR_VALIDATE}" >"${scratch}/force-docker.out" 2>&1 || true
if grep -q 'native-cli-invoked' "${scratch}/force-docker.log"; then
  fail "STRUCTURIZR_FORCE_DOCKER=1 invoked native CLI even with docker available"
elif grep -q 'docker-invoked:' "${scratch}/force-docker.log"; then
  ok "STRUCTURIZR_FORCE_DOCKER=1 uses docker when available"
else
  # Repo may have zero matching workspaces in some checkouts; require the script
  # to document force-docker and refuse native. Check the script itself.
  if grep -qE 'STRUCTURIZR_FORCE_DOCKER' "${STRUCTURIZR_VALIDATE}" \
    && ! grep -qE 'native-cli-invoked' "${scratch}/force-docker.log"; then
    # If workspaces exist in this clone, docker must have been called.
    if compgen -G "${ROOT}/docs/architecture/structurizr/*/workspace.dsl" >/dev/null; then
      fail "STRUCTURIZR_FORCE_DOCKER=1 did not invoke docker despite workspaces present"
    else
      ok "STRUCTURIZR_FORCE_DOCKER path present; no workspaces to exercise docker"
    fi
  else
    fail "STRUCTURIZR_FORCE_DOCKER=1 neither invoked docker nor declares the force mode"
  fi
fi

echo "== contract test carries no digest literal =="
if grep -qE 'sha256:[0-9a-f]{64}' "${THIS_TEST}"; then
  fail "test-ci-hardening-b1.sh embeds a digest literal (violates single-source)"
else
  ok "contract test has no digest literal"
fi

echo "== derived release tag is not a second pin literal outside the pin file =="
# Scan B1 production/contract paths for the derived tag. Allow the pin file and
# Cargo.toml; ignore GitHub Actions SHA-pin version comments (# vX.Y.Z).
if python3 - "${EXPECTED_TAG}" "${ROOT}" <<'PY'
import re
import sys
from pathlib import Path

tag = sys.argv[1]
root = Path(sys.argv[2])
allow_files = {
    root / ".github" / "assay-release-tag",
    root / "Cargo.toml",
}
scan = [
    root / "scripts" / "ci" / "test-ci-hardening-b1.sh",
    root / "scripts" / "ci" / "read-assay-release-tag.sh",
    root / "scripts" / "ci" / "test-structurizr-export-docker.sh",
    root / "scripts" / "structurizr-cli-image.sh",
    root / "scripts" / "structurizr-validate.sh",
    root / "scripts" / "structurizr-export.sh",
    root / ".github" / "workflows" / "assay.yml",
    root / ".github" / "workflows" / "assay-security.yml",
    root / ".github" / "workflows" / "structurizr-validate.yml",
]
action_ver_comment = re.compile(
    r"^\s*(?:-[^\n]*|uses:[^\n]*)#\s*" + re.escape(tag) + r"\s*$"
)
# Variable/name references that mention the derived value only as EXPECTED_TAG=.
expected_assign = re.compile(r"^EXPECTED_TAG=")
bad = []
for path in scan:
    if not path.exists() or path in allow_files:
        continue
    for i, line in enumerate(path.read_text().splitlines(), 1):
        if tag not in line:
            continue
        if action_ver_comment.match(line):
            continue
        if expected_assign.match(line.strip()):
            continue
        # Allow printing/comparing the derived variable, not a quoted literal pin.
        if f'"{tag}"' in line or f"'{tag}'" in line or f"{tag}\\n" in line or line.strip() == tag:
            bad.append(f"{path.relative_to(root)}:{i}:{line.strip()}")
            continue
        # Bare occurrence in prose/example (e.g. "e.g. <tag>") still counts.
        if re.search(r"(?:e\.g\.|example|fixture|want|got|is)\s+" + re.escape(tag), line):
            bad.append(f"{path.relative_to(root)}:{i}:{line.strip()}")
            continue
        if re.search(r"\b" + re.escape(tag) + r"\b", line):
            # References via ${EXPECTED_TAG} are fine.
            if "EXPECTED_TAG" in line and tag not in line.replace("${EXPECTED_TAG}", "").replace("$EXPECTED_TAG", ""):
                continue
            bad.append(f"{path.relative_to(root)}:{i}:{line.strip()}")
if bad:
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
PY
then
  ok "no stray ${EXPECTED_TAG} release-pin literals outside .github/assay-release-tag"
else
  fail "derived tag ${EXPECTED_TAG} appears as a release-pin literal outside the pin file"
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

echo "== required CI workflow actively runs both hardening contracts =="
# Without a workflow invocation, every GitHub check can stay green while these
# regressions return. The scope job is always required by the CI aggregator.
if python3 - "${CI_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
match = re.search(r"(?ms)^  scope:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text)
if not match:
    sys.exit("scope job missing from ci.yml")
section = match.group(1)
# Reject if/continue-on-error on the hardening step itself.
step = re.search(
    r"(?ms)^      - name: Verify CI hardening contracts\n(?P<body>(?:        .+\n)+)",
    section,
)
if not step:
    sys.exit("scope job missing 'Verify CI hardening contracts' step")
body = step.group("body")
for forbidden in ("if:", "continue-on-error:"):
    if re.search(rf"(?m)^        {re.escape(forbidden)}", body):
        sys.exit(f"hardening step must not use {forbidden}")
active = [
    line.strip()
    for line in body.splitlines()
    if line.startswith("          ") and not line.lstrip().startswith("#")
]
required = (
    "bash scripts/ci/test-ci-hardening-b1.sh",
    "bash scripts/ci/test-structurizr-export-docker.sh",
)
missing = [cmd for cmd in required if cmd not in active]
if missing:
    sys.exit("active commands missing: " + ", ".join(missing))
sys.exit(0)
PY
then
  ok "ci.yml scope job actively runs both hardening contract scripts"
else
  fail "ci.yml does not actively invoke both hardening contracts in the scope job"
fi

echo "== required CI workflow actively runs evidence-vocabulary self-test and live checker =="
if python3 - "${CI_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
match = re.search(r"(?ms)^  scope:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text)
if not match:
    sys.exit("scope job missing from ci.yml")
section = match.group(1)
step = re.search(
    r"(?ms)^      - name: Evidence vocabulary guard\n(?P<body>(?:        .+\n)+)",
    section,
)
if not step:
    sys.exit("scope job missing 'Evidence vocabulary guard' step")
body = step.group("body")
for forbidden in ("if:", "continue-on-error:"):
    if re.search(rf"(?m)^        {re.escape(forbidden)}", body):
        sys.exit(f"evidence-vocabulary step must not use {forbidden}")
active = [
    line.strip()
    for line in body.splitlines()
    if line.startswith("          ") and not line.lstrip().startswith("#")
]
required = (
    "bash scripts/ci/test-evidence-vocabulary.sh",
    "python3 scripts/ci/check-evidence-vocabulary.py",
)
missing = [cmd for cmd in required if cmd not in active]
if missing:
    sys.exit("active commands missing: " + ", ".join(missing))
sys.exit(0)
PY
then
  ok "ci.yml scope job actively runs evidence-vocabulary self-test and live checker"
else
  fail "ci.yml does not actively invoke evidence-vocabulary in the scope job"
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "ci-hardening-b1 contract: ${failures} failure(s)" >&2
  exit 1
fi

echo "ci-hardening-b1 contract: PASS"
