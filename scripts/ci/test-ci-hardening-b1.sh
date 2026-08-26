#!/usr/bin/env bash
# Wave B1 contract: single-source Assay release-tag pin, Structurizr image pin,
# timeouts on touched jobs, and a version-independent CI infra doc header.
#
# The install pin names the latest published release and may trail the source
# version during release preparation. Digests live only in the Structurizr pin
# file — never as test or consumer literals.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PIN_FILE="${ROOT}/.github/assay-release-tag"
READER="${ROOT}/scripts/ci/read-assay-release-tag.sh"
RELEASE_PIN_CHECK="${ROOT}/scripts/ci/check-assay-release-pin.sh"
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

echo "== checked-in install pin is valid and does not lead workspace =="
if [[ -f "${PIN_FILE}" ]]; then
  assay_pin="$(tr -d '[:space:]' <"${PIN_FILE}")"
  if "${RELEASE_PIN_CHECK}" >/dev/null; then
    ok "pin file ${assay_pin} is valid for the workspace release line"
  else
    fail "pin file ${assay_pin} is invalid for the workspace release line"
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
  if [[ "${got}" == "${assay_pin:-}" ]]; then
    ok "reader returns checked-in pin ${got}"
  else
    fail "reader returned '${got}', expected checked-in pin '${assay_pin:-<missing>}'"
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

echo "== install and workspace tags are not second pin literals =="
# Scan B1 production/contract paths for both release-line values. During release
# preparation they differ; neither may become a second install-pin source.
if python3 - "${assay_pin}" "${ROOT}" <<'PY'
import re
import sys
from pathlib import Path

pin_tag = sys.argv[1]
root = Path(sys.argv[2])
sys.path.insert(0, str(root / "scripts" / "ci" / "lib"))
from workspace_version import read_workspace_version

tags = {pin_tag, "v" + read_workspace_version(root / "Cargo.toml")}
allow_files = {
    root / ".github" / "assay-release-tag",
    root / "Cargo.toml",
}
scan = [
    root / "scripts" / "ci" / "test-ci-hardening-b1.sh",
    root / "scripts" / "ci" / "check-assay-release-pin.sh",
    root / "scripts" / "ci" / "read-assay-release-tag.sh",
    root / "scripts" / "ci" / "test-structurizr-export-docker.sh",
    root / "scripts" / "structurizr-cli-image.sh",
    root / "scripts" / "structurizr-validate.sh",
    root / "scripts" / "structurizr-export.sh",
    root / ".github" / "workflows" / "assay.yml",
    root / ".github" / "workflows" / "assay-security.yml",
    root / ".github" / "workflows" / "structurizr-validate.yml",
]
bad = []
for path in scan:
    if not path.exists() or path in allow_files:
        continue
    for i, line in enumerate(path.read_text().splitlines(), 1):
        for tag in tags:
            if tag not in line:
                continue
            action_ver_comment = re.compile(
                r"^\s*(?:-[^\n]*|uses:[^\n]*)#\s*" + re.escape(tag) + r"\s*$"
            )
            if action_ver_comment.match(line):
                continue
            if f'"{tag}"' in line or f"'{tag}'" in line or f"{tag}\\n" in line or line.strip() == tag:
                bad.append(f"{path.relative_to(root)}:{i}:{line.strip()}")
                continue
            # Bare occurrence in prose/example (e.g. "e.g. <tag>") still counts.
            if re.search(r"(?:e\.g\.|example|fixture|want|got|is)\s+" + re.escape(tag), line):
                bad.append(f"{path.relative_to(root)}:{i}:{line.strip()}")
                continue
            if re.search(r"\b" + re.escape(tag) + r"\b", line):
                bad.append(f"{path.relative_to(root)}:{i}:{line.strip()}")
if bad:
    print("\n".join(bad), file=sys.stderr)
    sys.exit(1)
PY
then
  ok "no stray install/workspace release-pin literals outside .github/assay-release-tag"
else
  fail "install/workspace tag appears as a release-pin literal outside the pin file"
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
# regressions return. The final CI job is the required aggregator.
if python3 - "${CI_WF}" <<'PY'
import re
import sys

text = open(sys.argv[1]).read()
match = re.search(r"(?ms)^  ci:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text)
if not match:
    sys.exit("ci job missing from ci.yml")
section = match.group(1)
# Reject if/continue-on-error on the hardening step itself.
heading = "      - name: Verify CI hardening contracts\n"
start = section.find(heading)
if start < 0:
    sys.exit("ci job missing 'Verify CI hardening contracts' step")
rest = section[start + len(heading):]
nxt = re.search(r"(?m)^      - ", rest)
body = rest if nxt is None else rest[:nxt.start()]
for forbidden in ("if:", "continue-on-error:"):
    if re.search(rf"(?m)^        {re.escape(forbidden)}", body):
        sys.exit(f"hardening step must not use {forbidden}")
required_env = ("GH_TOKEN: ${{ github.token }}",)
missing_env = [entry for entry in required_env if entry not in body]
if missing_env:
    sys.exit("hardening step environment missing: " + ", ".join(missing_env))
run_at = body.find("        run: |\n")
if run_at < 0:
    sys.exit("hardening step missing run script")
script = body[run_at + len("        run: |\n"):]
active = [
    line.strip()
    for line in script.splitlines()
    if line.startswith("          ") and not line.lstrip().startswith("#")
]
required = (
    "set -euo pipefail",
    "bash scripts/ci/test-check-assay-release-pin.sh",
    "bash scripts/ci/check-assay-release-pin.sh --published",
    "bash scripts/ci/test-check-assay-action-pin.sh",
    "bash scripts/ci/check-assay-action-pin.sh",
    "bash scripts/ci/check-assay-action-pin.sh --published",
    "bash scripts/ci/test-ci-hardening-b1.sh",
    "bash scripts/ci/test-structurizr-export-docker.sh",
    "python3 scripts/ci/check-conformance-inventory-callsite.py",
    "python3 scripts/ci/test-conformance-inventory-callsite.py",
)
if active != list(required):
    sys.exit("active hardening step body must be exactly %r, got %r" %
             (list(required), active))
sys.exit(0)
PY
then
  ok "ci.yml ci job actively runs both hardening contract scripts"
else
  fail "ci.yml does not actively invoke both hardening contracts in the ci job"
fi

echo "== required CI workflow command reachability (structural, not a runtime witness) =="
# One rule: a required command is reachable iff it appears as the exact
# canonical active line. A direct active `exit`/`return` command, with or
# without shell arguments (including quotes and expansions), makes later
# lines unreachable. Identifiers like `exit_status` and quoted prose are
# not terminators. A genuine short-circuit (`false && cmd`, `true || cmd`)
# makes only that skipped operand unreachable; later lines are still
# scanned. `true && cmd` still runs cmd in Bash, so it is a canonical-form
# miss, not a reachability bypass.
# Structural only: not a hosted execution witness.
if python3 - "${CI_WF}" <<'PY'
import re
import sys

TERMINATOR_RE = re.compile(r"^(?:exit|return)(?:\s+.*)?$")
SHORT_CIRCUIT_RE = re.compile(r"^(?:false\s+&&|true\s+\|\|)\s+(.+)$")

def command_reachability_problems(active, required):
    """Return problems if any required command is not structurally reachable.

    Canonical form is exact-line identity. A direct `exit`/`return` command
    (optional arguments, including quotes and expansions) stops later lines.
    Short-circuit skips only its operand. `true && cmd` executes cmd and is
    reported as not canonical, never as unreachable.
    """
    reachable = []
    terminator = None
    skipped_by = {}
    for line in active:
        if terminator is not None:
            break
        if TERMINATOR_RE.fullmatch(line):
            terminator = line
            break
        skipped = SHORT_CIRCUIT_RE.fullmatch(line)
        if skipped:
            operand = skipped.group(1)
            skipped_by.setdefault(operand, line)
            continue
        if line == ":":
            continue
        reachable.append(line)
    problems = []
    for cmd in required:
        if cmd in reachable:
            continue
        if cmd in skipped_by:
            problems.append(
                "required command unreachable after short-circuit "
                f"{skipped_by[cmd]!r}: {cmd}"
            )
        elif terminator is not None:
            problems.append(
                f"required command unreachable after {terminator!r}: {cmd}"
            )
        else:
            problems.append(
                f"required command missing, neutralized, or not canonical: {cmd}"
            )
    return problems

def scope_section(text):
    match = re.search(r"(?ms)^  scope:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text)
    if not match:
        sys.exit("scope job missing from ci.yml")
    return match.group(1)

def step_body(section, heading, step_re):
    step = re.search(step_re, section)
    if not step:
        sys.exit(f"scope job missing {heading!r} step")
    body = step.group("body")
    for forbidden in ("if:", "continue-on-error:"):
        if re.search(rf"(?m)^        {re.escape(forbidden)}", body):
            sys.exit(f"{heading} step must not use {forbidden}")
    return body

def active_run_lines(body):
    active = []
    for line in body.splitlines():
        if not line.startswith("          "):
            continue
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        active.append(stripped)
    return active

EVIDENCE_HEADING = "Evidence vocabulary guard"
EVIDENCE_RE = (
    r"(?ms)^      - name: Evidence vocabulary guard\n(?P<body>(?:        .+\n)+)"
)
EVIDENCE_REQUIRED = (
    "bash scripts/ci/test-evidence-vocabulary.sh",
    "python3 scripts/ci/check-evidence-vocabulary.py",
)
EVIDENCE_NEEDLE = "          bash scripts/ci/test-evidence-vocabulary.sh\n"
PUBLISHED_HEADING = "Published-numbers projection contract"
PUBLISHED_RE = (
    r"(?m)^      - name: Published-numbers projection contract\n"
    r"(?P<body>(?:        .+\n)+)(?:\n*)(?=^      - |\Z)"
)
PUBLISHED_REQUIRED = (
    "set -euo pipefail",
    "python3 conformance/tests/test_published_numbers_provenance.py",
    "python3 conformance/tests/test_published_numbers_guard.py",
    "python3 conformance/adequacy/check_published_numbers.py",
)
PUBLISHED_NEEDLE = (
    "          python3 conformance/tests/test_published_numbers_provenance.py\n"
)

text = open(sys.argv[1]).read()
section = scope_section(text)
evidence_body = step_body(section, EVIDENCE_HEADING, EVIDENCE_RE)
evidence_active = active_run_lines(evidence_body)
evidence_problems = command_reachability_problems(
    evidence_active, EVIDENCE_REQUIRED
)
if evidence_problems:
    sys.exit("; ".join(evidence_problems))

setup_at = section.find("uses: actions/setup-python")
step_at = section.find("- name: Published-numbers projection contract")
if setup_at < 0 or step_at < 0 or step_at < setup_at:
    sys.exit("published-numbers step must follow actions/setup-python")
published_body = step_body(section, PUBLISHED_HEADING, PUBLISHED_RE)
published_active = active_run_lines(published_body)
if published_active != list(PUBLISHED_REQUIRED):
    sys.exit(
        "active published-numbers step body must be exactly %r, got %r"
        % (list(PUBLISHED_REQUIRED), published_active)
    )
published_problems = command_reachability_problems(
    published_active, PUBLISHED_REQUIRED
)
if published_problems:
    sys.exit("; ".join(published_problems))

# `true && cmd` executes cmd. Exact-form miss, not a reachability bypass.
true_and_problems = command_reachability_problems(
    [f"true && {EVIDENCE_REQUIRED[0]}", EVIDENCE_REQUIRED[1]],
    EVIDENCE_REQUIRED,
)
if not true_and_problems:
    sys.exit("true && prefix must fail canonical form")
if any("unreachable" in p for p in true_and_problems):
    sys.exit(
        "true && cmd still executes; must not be reported as unreachable: "
        + "; ".join(true_and_problems)
    )
if not any("not canonical" in p for p in true_and_problems):
    sys.exit(
        "true && cmd must be named as a canonical-form miss: "
        + "; ".join(true_and_problems)
    )

def _quoted_or_expanded_terminates(line):
    problems = command_reachability_problems(
        [line, EVIDENCE_REQUIRED[0], EVIDENCE_REQUIRED[1]],
        EVIDENCE_REQUIRED,
    )
    joined = "; ".join(problems)
    if not problems:
        sys.exit(
            f"{line!r} must make later required commands unreachable (false-clean)"
        )
    for cmd in EVIDENCE_REQUIRED:
        if cmd not in joined or "unreachable" not in joined:
            sys.exit(
                f"{line!r} must mark later required commands unreachable: "
                + joined
            )

_quoted_or_expanded_terminates('exit "0"')
_quoted_or_expanded_terminates('exit "$?"')
_quoted_or_expanded_terminates('return "$code"')

# Direct exit/return only. Identifiers and quoted prose are not terminators.
for decoy, label in (
    ("exit_status=1", "exit_status assignment"),
    ('echo "please exit now"', "quoted text containing exit"),
):
    decoy_problems = command_reachability_problems(
        [decoy, EVIDENCE_REQUIRED[0], EVIDENCE_REQUIRED[1]],
        EVIDENCE_REQUIRED,
    )
    if decoy_problems:
        sys.exit(
            f"{label} must not terminate later lines: "
            + "; ".join(decoy_problems)
        )

# Optional short-circuit skips only that operand; later required lines run.
later_pair = ("bash required-one.sh", "bash required-two.sh")
optional_later = command_reachability_problems(
    ["false && echo optional", later_pair[0], later_pair[1]],
    later_pair,
)
if optional_later:
    sys.exit(
        "optional false && before two required commands must leave both "
        "reachable: " + "; ".join(optional_later)
    )
optional_and = command_reachability_problems(
    ["false && echo optional", EVIDENCE_REQUIRED[0], EVIDENCE_REQUIRED[1]],
    EVIDENCE_REQUIRED,
)
if optional_and:
    sys.exit(
        "optional false && must leave later required commands reachable: "
        + "; ".join(optional_and)
    )
optional_or = command_reachability_problems(
    ["true || echo optional", EVIDENCE_REQUIRED[0], EVIDENCE_REQUIRED[1]],
    EVIDENCE_REQUIRED,
)
if optional_or:
    sys.exit(
        "optional true || must leave later required commands reachable: "
        + "; ".join(optional_or)
    )

def _short_circuit_only_operand(prefix, required):
    active = [f"{prefix} {required[0]}", required[1]]
    problems = command_reachability_problems(active, required)
    joined = "; ".join(problems)
    if not problems:
        sys.exit(f"{prefix} REQUIRED must fail that required command")
    if required[0] not in joined or "unreachable" not in joined:
        sys.exit(
            f"{prefix} REQUIRED must mark only that operand unreachable: "
            + joined
        )
    if required[1] in joined:
        sys.exit(
            f"{prefix} REQUIRED must not mark the later command unreachable: "
            + joined
        )

_short_circuit_only_operand("false &&", later_pair)
_short_circuit_only_operand("true ||", later_pair)
_short_circuit_only_operand("false &&", EVIDENCE_REQUIRED)
_short_circuit_only_operand("true ||", EVIDENCE_REQUIRED)

def apply_mutation(source, needle, kind):
    if needle not in source:
        sys.exit(f"{kind} mutation needle missing: {needle!r}")
    if kind == "exit 0":
        return source.replace(needle, "          exit 0\n" + needle, 1)
    if kind == "exit":
        return source.replace(needle, "          exit\n" + needle, 1)
    if kind == "return 0":
        return source.replace(needle, "          return 0\n" + needle, 1)
    if kind == 'exit "0"':
        return source.replace(needle, '          exit "0"\n' + needle, 1)
    if kind == 'exit "$?"':
        return source.replace(needle, '          exit "$?"\n' + needle, 1)
    if kind == 'return "$code"':
        return source.replace(needle, '          return "$code"\n' + needle, 1)
    if kind == ": neutralization":
        return source.replace(needle, "          :\n", 1)
    if kind == "false && skip":
        return source.replace(needle, "          false && " + needle.lstrip(), 1)
    if kind == "true || skip":
        return source.replace(needle, "          true || " + needle.lstrip(), 1)
    if kind == "comment-out":
        return source.replace(needle, "          # " + needle.lstrip(), 1)
    if kind == "deletion":
        return source.replace(needle, "", 1)
    sys.exit(f"unknown mutation {kind!r}")

MUTATIONS = (
    "exit 0",
    "exit",
    "return 0",
    'exit "0"',
    'exit "$?"',
    'return "$code"',
    ": neutralization",
    "false && skip",
    "true || skip",
    "comment-out",
    "deletion",
)
TARGETS = (
    (EVIDENCE_HEADING, EVIDENCE_RE, EVIDENCE_REQUIRED, EVIDENCE_NEEDLE),
    (PUBLISHED_HEADING, PUBLISHED_RE, PUBLISHED_REQUIRED, PUBLISHED_NEEDLE),
)
for heading, step_re, required, needle in TARGETS:
    for kind in MUTATIONS:
        control = command_reachability_problems(
            active_run_lines(step_body(scope_section(text), heading, step_re)),
            required,
        )
        if control:
            sys.exit(f"green control red before {heading} {kind}: " + "; ".join(control))
        mutated = apply_mutation(text, needle, kind)
        if mutated == text:
            sys.exit(f"{heading} {kind} mutation was a no-op")
        problems = command_reachability_problems(
            active_run_lines(
                step_body(scope_section(mutated), heading, step_re)
            ),
            required,
        )
        if not problems:
            sys.exit(
                f"{heading}: {kind} left required commands reachable (false-clean)"
            )
        if kind in ("false && skip", "true || skip"):
            mutated_cmd = needle.strip()
            joined = "; ".join(problems)
            if mutated_cmd not in joined:
                sys.exit(
                    f"{heading}: {kind} must fail the skipped operand: "
                    + joined
                )
            later = False
            for line in active_run_lines(
                step_body(scope_section(mutated), heading, step_re)
            ):
                if later and line in required and line in joined:
                    sys.exit(
                        f"{heading}: {kind} over-reported later command "
                        f"{line!r}: " + joined
                    )
                if line.startswith(("false && ", "true || ")) and line.endswith(
                    mutated_cmd
                ):
                    later = True
        control = command_reachability_problems(
            active_run_lines(step_body(scope_section(text), heading, step_re)),
            required,
        )
        if control:
            sys.exit(f"green control red after {heading} {kind}: " + "; ".join(control))
sys.exit(0)
PY
then
  ok "ci.yml scope job required commands are structurally reachable"
else
  fail "ci.yml required command reachability failed"
fi

echo "== evidence-vocabulary NUL policy is content-first with a stale-checked binary allowlist =="
if python3 - "${ROOT}" <<'PY'
import importlib.util
import inspect
import io
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout
from pathlib import Path

root = Path(sys.argv[1])
checker = root / "scripts/ci/check-evidence-vocabulary.py"
spec = importlib.util.spec_from_file_location("evidence_vocabulary", checker)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)

positive = root / "tests/fixtures/ci/content-first/extensionless-text"
negative = root / "tests/fixtures/ci/content-first/genuine.bin"
if not positive.is_file() or positive.suffix:
    sys.exit("extensionless textual positive fixture missing")
if b"\0" in positive.read_bytes():
    sys.exit("positive fixture must be textual (no NUL) so live CI stays green")
if not negative.is_file() or negative.suffix != ".bin":
    sys.exit("genuine binary negative fixture missing")
if b"\0" not in negative.read_bytes():
    sys.exit("negative fixture must contain NUL")

rekor_rel = "crates/assay-registry/src/rekor.rs"
rekor = (root / rekor_rel).read_bytes()
allow_attr = next(
    n for n in dir(module) if n.startswith("ALLOWED_") and n.endswith("_USES")
)
allow_map = getattr(module, allow_attr)

def run_tree(files, **kwargs):
    with tempfile.TemporaryDirectory() as tmp:
        dest = Path(tmp)
        (dest / "crates/assay-registry/src").mkdir(parents=True)
        (dest / "crates/assay-registry/src/rekor.rs").write_bytes(rekor)
        for rel, data in files.items():
            path = dest / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        subprocess.run(["git", "init", "-q"], cwd=dest, check=True)
        subprocess.run(["git", "add", "-A", "--", "."], cwd=dest, check=True)
        allow = {rekor_rel: allow_map[rekor_rel]}
        buf = io.StringIO()
        with redirect_stdout(buf):
            rc = module.check_tree(dest, allow, identifiers={}, **kwargs)
        return rc, buf.getvalue()

rc, out = run_tree({"extensionless-text": b"plain text with a NUL\x00byte\n"})
if rc == 0 or "extensionless-text" not in out:
    sys.exit(
        "FAIL: NUL in an extensionless tracked file must fail closed:\n" + out
    )

rc, out = run_tree({"notes.mdx": b"docs with a NUL\x00inside\n"})
if rc == 0 or "notes.mdx" not in out:
    sys.exit("FAIL: NUL in an unlisted textual suffix must fail closed:\n" + out)

rc, out = run_tree(
    {"tests/fixtures/ci/content-first/genuine.bin": negative.read_bytes()}
)
if rc != 0:
    sys.exit(
        "FAIL: path-bound genuine binary with matching magic must pass:\n" + out
    )

rc, out = run_tree({"genuine.bin": negative.read_bytes()})
if rc == 0 or "genuine.bin" not in out:
    sys.exit(
        "FAIL: ELF magic at a non-allowlisted path must fail closed:\n" + out
    )

rc, out = run_tree({"disguise.bin": b"plain text with a NUL\x00byte\n"})
if rc == 0 or "disguise.bin" not in out:
    sys.exit(
        "FAIL: NUL text named .bin without binary magic must fail closed:\n" + out
    )

png_sig = bytes([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
token = "M" + "erkle"
hostile = png_sig + b"\x00" + f"run_root is a {token} root\n".encode()
rc, out = run_tree({"hostile-png-prefix": hostile})
if rc == 0 or "hostile-png-prefix" not in out:
    sys.exit(
        "FAIL: PNG magic prefix + NUL + forbidden claim at a non-allowlisted "
        "path must fail closed:\n" + out
    )

if getattr(module, "BINARY_ALLOWLIST_SUFFIXES", None):
    sys.exit("FAIL: suffix binary escape must not exist")
if getattr(module, "BINARY_MAGIC", None) is not None:
    sys.exit("FAIL: global BINARY_MAGIC escape must not exist")
if getattr(module, "BINARY_ALLOWLIST_PATHS", None):
    sys.exit("FAIL: path-only binary OR-escape must not exist")
exceptions = getattr(module, "BINARY_EXCEPTIONS", None)
if not exceptions:
    sys.exit("FAIL: BINARY_EXCEPTIONS path-and-magic table missing")
path_class = getattr(module, "matches_path_class", None)
allow_bin = getattr(module, "is_allowlisted_binary", None)
stale = getattr(module, "binary_allowlist_staleness", None)
if not callable(path_class) or not callable(allow_bin) or not callable(stale):
    sys.exit("FAIL: path-class matcher, runtime allowance, or staleness missing")
if "matches_path_class" not in inspect.getsource(allow_bin):
    sys.exit("FAIL: runtime allowance must call matches_path_class")
if "matches_path_class" not in inspect.getsource(stale):
    sys.exit("FAIL: staleness must call matches_path_class")
matcher_src = inspect.getsource(path_class)
if "fnmatchcase" not in matcher_src or "fnmatch.fnmatch(" in matcher_src:
    sys.exit("FAIL: matches_path_class must use fnmatchcase, not fnmatch.fnmatch")
if "fnmatch.fnmatch(" in inspect.getsource(allow_bin) + inspect.getsource(stale):
    sys.exit("FAIL: must not use fnmatch.fnmatch on repository paths")

SEGMENT_PATTERN = "docs/assets/evidence-receipts-in-action/*/evidence.tar.gz"
SEGMENT_ONE = "docs/assets/evidence-receipts-in-action/a/evidence.tar.gz"
SEGMENT_DEEP = "docs/assets/evidence-receipts-in-action/a/b/evidence.tar.gz"
SEGMENT_CASE = "docs/assets/evidence-receipts-in-action/a/Evidence.tar.gz"
GZIP_NUL = b"\x1f\x8b\x00"


def assert_segment_bound(match, label):
    if not match(SEGMENT_ONE, SEGMENT_PATTERN):
        sys.exit(f"FAIL: {label}: one-segment path class must match")
    if match(SEGMENT_DEEP, SEGMENT_PATTERN):
        sys.exit(f"FAIL: {label}: extra-depth path must not match one-segment glob")
    if match(SEGMENT_CASE, SEGMENT_PATTERN):
        sys.exit(f"FAIL: {label}: case-changed path must not match")


assert_segment_bound(path_class, "matches_path_class")
assert_segment_bound(
    lambda rel, pat: allow_bin(rel, GZIP_NUL, ((pat, "gzip"),)),
    "is_allowlisted_binary",
)


def stale_hit(rel, pattern):
    with tempfile.TemporaryDirectory() as tmp:
        dest = Path(tmp)
        path = dest / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(GZIP_NUL)
        return not stale(dest, [path], exceptions=((pattern, "gzip"),))


assert_segment_bound(stale_hit, "binary_allowlist_staleness")

messages = stale(
    root,
    [root / "README.md"],
    exceptions=(("tests/fixtures/ci/content-first/missing.bin", "elf"),),
)
if not messages:
    sys.exit("FAIL: stale binary allowlist entries must fail")
joined = "\n".join(messages)
if "tests/fixtures/ci/content-first/missing.bin" not in joined:
    sys.exit("FAIL: stale path was not reported:\n" + joined)
sys.exit(0)
PY
then
  ok "NUL policy is content-first; stale binary allowlist entries fail"
else
  fail "evidence-vocabulary NUL policy contract failed"
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "ci-hardening-b1 contract: ${failures} failure(s)" >&2
  exit 1
fi

echo "ci-hardening-b1 contract: PASS"
