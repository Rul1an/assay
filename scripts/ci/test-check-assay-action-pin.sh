#!/usr/bin/env bash
# Mutation battery for the Assay consumer Action pin.
#
# The pin file is the only mutable execution authority. This test reads that
# file, copies the live tree into scratch, and mutates the copy. It does not
# restate the consumed commit as a second expected constant.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="${ROOT}/scripts/ci/check-assay-action-pin.sh"
READER="${ROOT}/scripts/ci/read-assay-action-pin.sh"
PIN_FILE="${ROOT}/.github/assay-action-pin"
FIXTURE="${ROOT}/scripts/ci/fixtures/assay-action-pin/action.yml"
PROVENANCE="${ROOT}/scripts/ci/fixtures/assay-action-pin/PROVENANCE"
ASSAY_WF="${ROOT}/.github/workflows/assay.yml"
ACTION_WF="${ROOT}/.github/workflows/action-v2-test.yml"
USER_FLOWS="${ROOT}/docs/AIcontext/user-flows.md"
CI_INTEGRATION="${ROOT}/docs/getting-started/ci-integration.md"
GITHUB_ACTION_DOC="${ROOT}/docs/guides/github-action.md"
CICD_STARTER="${ROOT}/packs/open/cicd-starter/README.md"
PINNED_ACTIONS="${ROOT}/docs/PINNED-ACTIONS.md"
CHANGELOG="${ROOT}/CHANGELOG.md"
DEPENDABOT="${ROOT}/.github/dependabot.yml"
CI_YML="${ROOT}/.github/workflows/ci.yml"
PRECOMMIT="${ROOT}/.pre-commit-config.yaml"
ASSAY_COMMIT="e65394d572d3fad649624ab3fa413be934b1d9fa"

scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

require_exists() {
  local path="$1"
  if [[ ! -f "${path}" ]]; then
    echo "missing required path: ${path}" >&2
    exit 1
  fi
}

copy_into() {
  local dest="$1"
  mkdir -p \
    "${dest}/.github/workflows" \
    "${dest}/scripts/ci/fixtures/assay-action-pin" \
    "${dest}/docs/AIcontext" \
    "${dest}/docs/getting-started" \
    "${dest}/docs/guides"
  cp "${PIN_FILE}" "${dest}/.github/assay-action-pin"
  cp "${FIXTURE}" "${dest}/scripts/ci/fixtures/assay-action-pin/action.yml"
  cp "${PROVENANCE}" "${dest}/scripts/ci/fixtures/assay-action-pin/PROVENANCE"
  if [[ -x "${CHECKER}" ]] && "${CHECKER}" --list-paths >/dev/null 2>&1; then
    while IFS= read -r rel; do
      [[ -z "${rel}" ]] && continue
      mkdir -p "${dest}/$(dirname "${rel}")"
      cp "${ROOT}/${rel}" "${dest}/${rel}"
    done < <("${CHECKER}" --list-paths)
  else
    cp "${ASSAY_WF}" "${dest}/.github/workflows/assay.yml"
    cp "${ACTION_WF}" "${dest}/.github/workflows/action-v2-test.yml"
    cp "${USER_FLOWS}" "${dest}/docs/AIcontext/user-flows.md"
    cp "${CI_INTEGRATION}" "${dest}/docs/getting-started/ci-integration.md"
    cp "${GITHUB_ACTION_DOC}" "${dest}/docs/guides/github-action.md"
  fi
}

run_checker_at() {
  local tree="$1"
  shift
  ASSAY_ACTION_TREE="${tree}" \
    ASSAY_ACTION_PIN_FILE="${tree}/.github/assay-action-pin" \
    ASSAY_ACTION_FIXTURE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/action.yml" \
    ASSAY_ACTION_PROVENANCE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/PROVENANCE" \
    "${CHECKER}" "$@"
}

expect_fail() {
  local name="$1"
  local expected="$2"
  local tree="$3"
  shift 3
  if run_checker_at "${tree}" "$@" >"${scratch}/out" 2>"${scratch}/err"; then
    echo "FAIL: ${name} stayed green; expected failure containing: ${expected}" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
  if ! grep -Fq -- "${expected}" "${scratch}/err"; then
    echo "FAIL: ${name} did not contain '${expected}':" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
  echo "ok    ${name} (owner gate failed)"
}

expect_ok() {
  local name="$1"
  shift
  if ! "$@" >"${scratch}/out" 2>"${scratch}/err"; then
    echo "FAIL: ${name} exited non-zero:" >&2
    cat "${scratch}/out" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
  echo "ok    ${name}"
}

mutate_once() {
  local path="$1" old="$2" new="$3"
  python3 - "$path" "$old" "$new" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old, new = sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
count = text.count(old)
if count != 1:
    raise SystemExit(f"mutation subject count is {count}, want 1: {old!r}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

check_hook_invokes_gate() {
  python3 - "${PRECOMMIT}" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
if "- id: assay-action-consumer-pin" not in text:
    raise SystemExit("pre-commit is missing assay-action-consumer-pin")
block = text.split("- id: assay-action-consumer-pin", 1)[1].split("\n      - id:", 1)[0]
if "scripts/ci/check-assay-action-pin.sh" not in block:
    raise SystemExit("pre-commit hook does not call scripts/ci/check-assay-action-pin.sh")
if "scripts/ci/test-check-assay-action-pin.sh" not in block:
    raise SystemExit("pre-commit hook does not call scripts/ci/test-check-assay-action-pin.sh")
match = re.search(r"^[ \t]*files:[ \t]*(.+)$", block, re.MULTILINE)
if match is None:
    raise SystemExit("assay-action-consumer-pin hook has no files selector")
pattern = match.group(1).strip()
required = (
    ".github/assay-action-pin",
    ".github/workflows/assay.yml",
    ".github/workflows/action-v2-test.yml",
    ".github/workflows/release.yml",
    "scripts/ci/read-assay-action-pin.sh",
    "scripts/ci/check-assay-action-pin.sh",
    "scripts/ci/test-check-assay-action-pin.sh",
    "scripts/ci/fixtures/assay-action-pin/action.yml",
    "scripts/ci/fixtures/assay-action-pin/PROVENANCE",
    "docs/AIcontext/user-flows.md",
    "docs/getting-started/ci-integration.md",
    "docs/guides/github-action.md",
    "docs/guides/rollout-template.md",
    "docs/index.md",
    "docs/PINNED-ACTIONS.md",
    "CHANGELOG.md",
    ".github/dependabot.yml",
    "packs/open/cicd-starter/README.md",
    "crates/assay-cli/src/templates.rs",
    ".pre-commit-config.yaml",
)
missing = [path for path in required if re.search(pattern, path) is None]
if missing:
    raise SystemExit(f"assay-action-consumer-pin hook does not trigger for: {', '.join(missing)}")
PY
}

require_exists "${CHECKER}"
require_exists "${READER}"
require_exists "${PIN_FILE}"
require_exists "${FIXTURE}"
require_exists "${PROVENANCE}"
require_exists "${ASSAY_WF}"
require_exists "${ACTION_WF}"
require_exists "${USER_FLOWS}"
require_exists "${CI_INTEGRATION}"
require_exists "${GITHUB_ACTION_DOC}"
require_exists "${CICD_STARTER}"
require_exists "${PINNED_ACTIONS}"
require_exists "${CHANGELOG}"
require_exists "${DEPENDABOT}"
require_exists "${CI_YML}"
require_exists "${PRECOMMIT}"

check_consumer_compat() {
  python3 - "$1" "$2" "$3" <<'PY'
import sys
from pathlib import Path

dependabot = Path(sys.argv[1]).read_text(encoding="utf-8")
pinned = Path(sys.argv[2]).read_text(encoding="utf-8")
changelog = Path(sys.argv[3]).read_text(encoding="utf-8")
errors = []
if 'assay-dev/assay-action' in dependabot:
    errors.append("Dependabot ignore names assay-dev/assay-action; want Rul1an/assay-action")
if 'dependency-name: "Rul1an/assay-action"' not in dependabot:
    errors.append("Dependabot does not ignore Rul1an/assay-action")
if ".github/assay-action-pin" not in pinned or "not a second place to change" not in pinned:
    errors.append("PINNED-ACTIONS.md does not record the pin-file exception")
if "Do not move floating `v3`" not in pinned or "Do not move frozen `v2`" not in pinned:
    errors.append("PINNED-ACTIONS.md does not record Assay-side rollback")
start = changelog.find("## [Unreleased]")
if start < 0:
    errors.append("CHANGELOG.md has no Unreleased section")
else:
    rest = changelog[start:]
    nxt = rest.find("\n## [", 1)
    unreleased = rest if nxt < 0 else rest[:nxt]
    for needle in (
        "mixed Action migration",
        "literal `false`",
        "sandbox-command",
        "v3.0.1 to v3.0.2",
        "not measured",
    ):
        if needle not in unreleased:
            errors.append(f"CHANGELOG Unreleased does not name {needle!r}")
if errors:
    raise SystemExit("; ".join(errors))
PY
}

PIN="$("${READER}")"
if [[ ! "${PIN}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "reader did not return a 40-hex pin" >&2
  exit 1
fi
EXPECTED_USES="Rul1an/assay-action@${PIN}"

echo "== no-op control =="
expect_ok "control-is-green" "${CHECKER}"
expect_ok "reader-returns-pin" bash -c "test \"\$(\"${READER}\")\" = '${PIN}'"
python3 - "${FIXTURE}" "${PROVENANCE}" "${PIN}" <<'PY'
import hashlib
import sys
from pathlib import Path

fixture = Path(sys.argv[1]).read_bytes()
provenance = Path(sys.argv[2]).read_text(encoding="utf-8")
pin = sys.argv[3]
digest = hashlib.sha256(fixture).hexdigest()
commit = None
recorded = None
for line in provenance.splitlines():
    line = line.strip()
    if line.startswith("commit="):
        commit = line.split("=", 1)[1].strip()
    elif line.startswith("sha256="):
        recorded = line.split("=", 1)[1].strip()
if commit != pin:
    raise SystemExit(f"provenance commit {commit} != pin {pin}")
if recorded != digest:
    raise SystemExit(f"provenance sha256 {recorded} != fixture {digest}")
PY
echo "ok    fixture-digest-matches-provenance"

python3 - "${ASSAY_WF}" "${ACTION_WF}" "${PIN}" <<'PY'
import re
import sys
from pathlib import Path

pin = sys.argv[3]
expected = f"Rul1an/assay-action@{pin}"
local = re.compile(r"uses:\s*\./assay-action")
expr = re.compile(r"uses:\s*\$\{\{")
for path in (Path(sys.argv[1]), Path(sys.argv[2])):
    text = path.read_text(encoding="utf-8")
    if local.search(text):
        raise SystemExit(f"{path}: still uses ./assay-action")
    if expr.search(text):
        raise SystemExit(f"{path}: uses is derived from an expression")
    if f"uses: {expected}" not in text:
        raise SystemExit(f"{path}: missing literal uses: {expected}")
PY
echo "ok    live-workflow-uses-are-literal-pin"

expect_ok "pre-commit-calls-owner-gate" check_hook_invokes_gate
if ! grep -Fq 'bash scripts/ci/check-assay-action-pin.sh --published' "${CI_YML}"; then
  echo "ci.yml does not invoke check-assay-action-pin.sh --published" >&2
  exit 1
fi
echo "ok    ci-invokes-live-published-byte-check"
expect_ok "consumer-compat-live" check_consumer_compat \
  "${DEPENDABOT}" "${PINNED_ACTIONS}" "${CHANGELOG}"
if ! "${CHECKER}" --list-paths | grep -Fxq 'packs/open/cicd-starter/README.md'; then
  echo "owner snippet list omits packs/open/cicd-starter/README.md" >&2
  exit 1
fi
echo "ok    cicd-starter-readme-is-inventoried"

echo "== nonexistent / non-40 pin =="
copy_into "${scratch}/non40"
printf '%s\n' 'v3.1.0' >"${scratch}/non40/.github/assay-action-pin"
expect_fail "non-40-pin" "want exactly one ^[0-9a-f]{40}$ line" "${scratch}/non40"

copy_into "${scratch}/short"
printf '%s\n' "${PIN:0:39}" >"${scratch}/short/.github/assay-action-pin"
expect_fail "short-pin" "want exactly one ^[0-9a-f]{40}$ line" "${scratch}/short"

echo "== snippet ref drift =="
copy_into "${scratch}/drift"
mutate_once \
  "${scratch}/drift/.github/workflows/assay.yml" \
  "uses: ${EXPECTED_USES}" \
  "uses: Rul1an/assay-action@0000000000000000000000000000000000000000"
expect_fail "snippet-ref-drift" "does not equal pin ${PIN}" "${scratch}/drift"

echo "== undeclared with: input =="
copy_into "${scratch}/undeclared"
mutate_once \
  "${scratch}/undeclared/.github/workflows/assay.yml" \
  "          version: \${{ steps.assay_tag.outputs.version }}" \
  "          version: \${{ steps.assay_tag.outputs.version }}
          undeclared_input: true"
expect_fail "undeclared-with-input" "undeclared input 'undeclared_input'" "${scratch}/undeclared"

echo "== local ./assay-action substitution =="
copy_into "${scratch}/local"
mutate_once \
  "${scratch}/local/.github/workflows/action-v2-test.yml" \
  "      - name: Test action with no bundles
        uses: ${EXPECTED_USES}" \
  "      - name: Test action with no bundles
        uses: ./assay-action"
expect_fail "local-assay-action-substitution" "uses: ./assay-action" "${scratch}/local"

echo "== wrong-repo SHA (Assay commit as if assay-action) =="
copy_into "${scratch}/wrong-repo"
mutate_once \
  "${scratch}/wrong-repo/docs/AIcontext/user-flows.md" \
  "        uses: Rul1an/assay-action@v3" \
  "        uses: Rul1an/assay/assay-action@${ASSAY_COMMIT} # v2"
expect_fail "wrong-repo-assay-commit" "${ASSAY_COMMIT}" "${scratch}/wrong-repo"

echo "== provenance commit drift with pin intact =="
copy_into "${scratch}/prov"
mutate_once \
  "${scratch}/prov/scripts/ci/fixtures/assay-action-pin/PROVENANCE" \
  "commit=${PIN}" \
  "commit=0000000000000000000000000000000000000000"
expect_fail "provenance-commit-drift" "does not equal pin" "${scratch}/prov"

echo "== pinned fixture byte drift =="
copy_into "${scratch}/bytes"
python3 - "${scratch}/bytes/scripts/ci/fixtures/assay-action-pin/action.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[0] ^= 0x01
path.write_bytes(bytes(data))
PY
expect_fail "fixture-byte-drift" "pinned fixture digest" "${scratch}/bytes"

echo "== coordinated fixture+digest drift vs published bytes =="
copy_into "${scratch}/coord"
cp "${FIXTURE}" "${scratch}/published-oracle.yml"
python3 - "${scratch}/coord/scripts/ci/fixtures/assay-action-pin/action.yml" \
  "${scratch}/coord/scripts/ci/fixtures/assay-action-pin/PROVENANCE" <<'PY'
from pathlib import Path
import hashlib
import sys

fixture = Path(sys.argv[1])
provenance = Path(sys.argv[2])
# Keep YAML parseable so offline consistency can stay green; only the
# published-byte compare should catch coordinated digest+fixture drift.
data = fixture.read_bytes() + b"\n# coordinated-fixture-digest-drift\n"
fixture.write_bytes(data)
digest = hashlib.sha256(data).hexdigest()
text = provenance.read_text(encoding="utf-8")
old = None
for line in text.splitlines():
    if line.startswith("sha256="):
        old = line.split("=", 1)[1].strip()
        break
if not old:
    raise SystemExit("provenance missing sha256=")
provenance.write_text(text.replace(f"sha256={old}", f"sha256={digest}", 1), encoding="utf-8")
PY
if run_checker_at "${scratch}/coord" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "ok    coordinated-offline-is-blind-without-published-bytes"
else
  echo "FAIL: coordinated fixture+digest should stay green offline" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
if ASSAY_ACTION_PUBLISHED_FILE="${scratch}/coord/scripts/ci/fixtures/assay-action-pin/action.yml" \
  run_checker_at "${scratch}/coord" --published >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: published check accepted the fixture as its own published bytes" >&2
  exit 1
fi
if ! grep -Fq "must not be the fixture file" "${scratch}/err"; then
  echo "FAIL: self-compare did not refuse fixture-as-published:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
echo "ok    published-bytes-must-not-be-the-fixture"
ASSAY_ACTION_PUBLISHED_FILE="${scratch}/published-oracle.yml" \
  expect_fail "coordinated-fixture-digest-vs-published" "does not match published action.yml" "${scratch}/coord" --published

echo "== snippet deletion from allowlisted doc =="
copy_into "${scratch}/delete-doc"
mutate_once \
  "${scratch}/delete-doc/docs/AIcontext/user-flows.md" \
  "        uses: Rul1an/assay-action@v3" \
  "        run: echo skipped"
expect_fail "doc-snippet-deleted" "no Rul1an/assay-action@v3 uses found" "${scratch}/delete-doc"

echo "== undeclared with: input on floating @v3 doc snippet =="
copy_into "${scratch}/doc-input"
mutate_once \
  "${scratch}/doc-input/docs/AIcontext/user-flows.md" \
  "          fail_on: error" \
  "          fail_on: error
          undeclared_doc_input: true"
expect_fail "doc-undeclared-with-input" "undeclared input 'undeclared_doc_input'" "${scratch}/doc-input"

echo "== undeclared with: input outside the original two-doc allowlist =="
copy_into "${scratch}/scope"
mutate_once \
  "${scratch}/scope/docs/guides/github-action.md" \
  "      - name: Verify evidence
        uses: Rul1an/assay-action@v3
        with:
          fail_on: error
          baseline_key: \${{ github.event.repository.name }}" \
  "      - name: Verify evidence
        uses: Rul1an/assay-action@v3
        with:
          fail_on: error
          undeclared_doc_input: true
          baseline_key: \${{ github.event.repository.name }}"
expect_fail "scope-outside-two-docs" "undeclared input 'undeclared_doc_input'" "${scratch}/scope"

echo "== unlisted active snippet outside the owner list =="
copy_into "${scratch}/unlisted"
mkdir -p "${scratch}/unlisted/docs/getting-started"
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/unlisted/docs/getting-started/installation.md"
expect_fail "unlisted-snippet-file" "is not on the owner snippet list" "${scratch}/unlisted"

echo "== unlisted oversize file with snippet after 1 MiB =="
copy_into "${scratch}/oversize"
mkdir -p "${scratch}/oversize/docs/getting-started"
python3 - "${scratch}/oversize/docs/getting-started/installation.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
limit = 1048576
snippet = b"```yaml\n- uses: Rul1an/assay-action@v3\n```\n"
path.write_bytes(b"x" * limit + snippet)
if path.stat().st_size <= limit:
    raise SystemExit("oversize mutation did not exceed 1048576 bytes")
PY
expect_fail "unlisted-oversize-post-limit-snippet" "exceeds 1048576-byte limit" "${scratch}/oversize"

echo "== Dependabot owner drift =="
cp "${DEPENDABOT}" "${scratch}/dependabot.yml"
mutate_once \
  "${scratch}/dependabot.yml" \
  'dependency-name: "Rul1an/assay-action"' \
  'dependency-name: "assay-dev/assay-action"'
if check_consumer_compat "${scratch}/dependabot.yml" "${PINNED_ACTIONS}" "${CHANGELOG}" \
  >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: dependabot-wrong-owner stayed green" >&2
  exit 1
fi
if ! grep -Fq "assay-dev/assay-action" "${scratch}/err"; then
  echo "FAIL: dependabot-wrong-owner did not name assay-dev/assay-action:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
echo "ok    dependabot-wrong-owner (owner gate failed)"

echo "== PINNED-ACTIONS pin-file exception dropped =="
cp "${PINNED_ACTIONS}" "${scratch}/PINNED-ACTIONS.md"
python3 - "${scratch}/PINNED-ACTIONS.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "not a second place to change"
if text.count(old) != 1:
    raise SystemExit(f"PINNED-ACTIONS exception subject count is {text.count(old)}")
path.write_text(text.replace(old, "the callsite remains the only pin", 1), encoding="utf-8")
PY
if check_consumer_compat "${DEPENDABOT}" "${scratch}/PINNED-ACTIONS.md" "${CHANGELOG}" \
  >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: pinned-actions-exception-dropped stayed green" >&2
  exit 1
fi
if ! grep -Fq "pin-file exception" "${scratch}/err"; then
  echo "FAIL: pinned-actions-exception-dropped did not name pin-file exception:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
echo "ok    pinned-actions-exception-dropped (owner gate failed)"

echo "== PINNED-ACTIONS rollback dropped =="
cp "${PINNED_ACTIONS}" "${scratch}/PINNED-ACTIONS-rollback.md"
mutate_once \
  "${scratch}/PINNED-ACTIONS-rollback.md" \
  "Do not move floating \`v3\`." \
  "Floating v3 may be moved."
if check_consumer_compat "${DEPENDABOT}" "${scratch}/PINNED-ACTIONS-rollback.md" "${CHANGELOG}" \
  >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: pinned-actions-rollback-dropped stayed green" >&2
  exit 1
fi
if ! grep -Fq "Assay-side rollback" "${scratch}/err"; then
  echo "FAIL: pinned-actions-rollback-dropped did not name Assay-side rollback:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
echo "ok    pinned-actions-rollback-dropped (owner gate failed)"

echo "== CHANGELOG mixed-migration paragraph dropped =="
cp "${CHANGELOG}" "${scratch}/CHANGELOG.md"
python3 - "${scratch}/CHANGELOG.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
path.write_text(text.replace("mixed Action migration", "additive Action update", 1), encoding="utf-8")
PY
if check_consumer_compat "${DEPENDABOT}" "${PINNED_ACTIONS}" "${scratch}/CHANGELOG.md" \
  >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: changelog-mixed-dropped stayed green" >&2
  exit 1
fi
if ! grep -Fq "mixed Action migration" "${scratch}/err"; then
  echo "FAIL: changelog-mixed-dropped did not name mixed Action migration:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
echo "ok    changelog-mixed-dropped (owner gate failed)"

echo "== flow-style with: mapping undeclared input =="
copy_into "${scratch}/flow-with"
mutate_once \
  "${scratch}/flow-with/.github/workflows/assay.yml" \
  "        with:
          version: \${{ steps.assay_tag.outputs.version }}" \
  "        with: { version: v5.4.0, undeclared_inline: true }"
expect_fail "flow-style-with-mapping" "undeclared input 'undeclared_inline'" "${scratch}/flow-with"

echo "== flow-style uses mapping undeclared input =="
copy_into "${scratch}/flow-uses"
mutate_once \
  "${scratch}/flow-uses/.github/workflows/assay.yml" \
  "      - name: Setup Assay
        uses: ${EXPECTED_USES}
        with:
          version: \${{ steps.assay_tag.outputs.version }}" \
  "      - name: Setup Assay
        uses: ${EXPECTED_USES}
        with:
          version: \${{ steps.assay_tag.outputs.version }}
      - { uses: ${EXPECTED_USES}, with: { undeclared_inline: true } }"
expect_fail "flow-style-uses-mapping" "undeclared input 'undeclared_inline'" "${scratch}/flow-uses"

echo "== fixture replacement between digest validation and published compare =="
copy_into "${scratch}/toctou"
cp "${FIXTURE}" "${scratch}/toctou-oracle.yml"
python3 - "${scratch}/toctou/scripts/ci/fixtures/assay-action-pin/action.yml" \
  "${scratch}/toctou/scripts/ci/fixtures/assay-action-pin/PROVENANCE" \
  "${scratch}/toctou/.github/workflows/assay.yml" <<'PY'
from pathlib import Path
import hashlib
import sys

fixture = Path(sys.argv[1])
provenance = Path(sys.argv[2])
workflow = Path(sys.argv[3])
text = fixture.read_text(encoding="utf-8")
needle = "inputs:\n  bundles:"
if text.count(needle) != 1:
    raise SystemExit(f"fixture inputs subject count is {text.count(needle)}")
fixture.write_text(
    text.replace(
        needle,
        "inputs:\n  evil_between_reads:\n    description: toctou\n    required: false\n    default: ''\n  bundles:",
        1,
    ),
    encoding="utf-8",
)
digest = hashlib.sha256(fixture.read_bytes()).hexdigest()
prov = provenance.read_text(encoding="utf-8")
old = None
for line in prov.splitlines():
    if line.startswith("sha256="):
        old = line.split("=", 1)[1].strip()
        break
if not old:
    raise SystemExit("provenance missing sha256=")
provenance.write_text(prov.replace(f"sha256={old}", f"sha256={digest}", 1), encoding="utf-8")
wf = workflow.read_text(encoding="utf-8")
old_wf = "          version: ${{ steps.assay_tag.outputs.version }}"
if wf.count(old_wf) != 1:
    raise SystemExit("workflow version subject count is %s" % wf.count(old_wf))
workflow.write_text(
    wf.replace(old_wf, old_wf + "\n          evil_between_reads: true", 1),
    encoding="utf-8",
)
PY
if run_checker_at "${scratch}/toctou" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "ok    toctou-offline-accepts-coordinated-fixture"
else
  echo "FAIL: coordinated toctou fixture should stay green offline" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
ASSAY_ACTION_FIXTURE_SWAP_FILE="${scratch}/toctou-oracle.yml" \
  ASSAY_ACTION_PUBLISHED_FILE="${scratch}/toctou-oracle.yml" \
  expect_fail "toctou-between-phase-swap" "does not match published action.yml" "${scratch}/toctou" --published

echo "== cicd-starter monorepo action snippet =="
copy_into "${scratch}/cicd"
mutate_once \
  "${scratch}/cicd/packs/open/cicd-starter/README.md" \
  "- uses: Rul1an/assay-action@v3" \
  "- uses: Rul1an/assay/assay-action@${ASSAY_COMMIT}"
expect_fail "cicd-starter-monorepo-snippet" "is the monorepo path" "${scratch}/cicd"

echo "== no-op control after mutations =="
expect_ok "control-stays-green-after-scratch-mutations" "${CHECKER}"

echo "assay action consumer pin contract: PASS"
