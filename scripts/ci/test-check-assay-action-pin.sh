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
    ASSAY_ACTION_RECIPE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd" \
    "${CHECKER}" "$@"
}

# Scratch repositories must not inherit this process's git environment. Under pre-commit,
# GIT_INDEX_FILE (and friends) point at the outer repository's temporary index, so a plain
# `git -C <scratch>` writes there instead, and `git worktree add` then fails with
# ".git/index: index file open failed: Not a directory". The variable list is shared rather
# than repeated, because the two hand-written copies had already drifted apart.
# shellcheck source=scripts/ci/lib/git-env.sh
. "$(dirname "${BASH_SOURCE[0]}")/lib/git-env.sh"

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

prepare_yaml_timeout_tree() {
  local dest="$1"
  copy_into "${dest}"
  mkdir -p "${dest}/scripts/ci" "${dest}/fake-bin"
  cp "${CHECKER}" "${dest}/scripts/ci/check-assay-action-pin.sh"
  cp "${READER}" "${dest}/scripts/ci/read-assay-action-pin.sh"
  python3 - "${dest}/scripts/ci/check-assay-action-pin.sh" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = "            timeout=30,"
if text.count(needle) != 1:
    raise SystemExit("expected exactly one YAML parser timeout")
path.write_text(
    text.replace(
        needle,
        '            timeout=5 if source == "pinned action.yml" else 30,',
    ),
    encoding="utf-8",
)
PY
  cat >"${dest}/fake-bin/ruby" <<'PY'
#!/usr/bin/env python3
import os
from pathlib import Path
import sys
import time

state = Path(os.environ["ASSAY_FAKE_RUBY_STATE"])
attempt = int(state.read_text(encoding="utf-8")) + 1 if state.exists() else 1
state.write_text(str(attempt), encoding="utf-8")
mode = os.environ["ASSAY_FAKE_RUBY_MODE"]
if mode == "persistent-timeout" or (mode == "transient-timeout" and attempt == 1):
    time.sleep(6)
elif mode == "parser-failure":
    print("synthetic parser failure", file=sys.stderr)
    raise SystemExit(2)
os.execv(os.environ["ASSAY_REAL_RUBY"], [os.environ["ASSAY_REAL_RUBY"], *sys.argv[1:]])
PY
  chmod +x "${dest}/fake-bin/ruby"
}

run_yaml_timeout_checker() {
  local tree="$1" mode="$2" state="$3"
  PATH="${tree}/fake-bin:${PATH}" \
    ASSAY_REAL_RUBY="${REAL_RUBY}" \
    ASSAY_FAKE_RUBY_MODE="${mode}" \
    ASSAY_FAKE_RUBY_STATE="${state}" \
    ASSAY_ACTION_TREE="${tree}" \
    ASSAY_ACTION_PIN_FILE="${tree}/.github/assay-action-pin" \
    ASSAY_ACTION_FIXTURE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/action.yml" \
    ASSAY_ACTION_PROVENANCE_FILE="${tree}/scripts/ci/fixtures/assay-action-pin/PROVENANCE" \
    "${tree}/scripts/ci/check-assay-action-pin.sh"
}

expect_attempts() {
  local name="$1" state="$2" expected="$3"
  local actual
  actual="$(cat "${state}")"
  if [[ "${actual}" != "${expected}" ]]; then
    echo "FAIL: ${name} made ${actual} parser attempts; expected ${expected}" >&2
    exit 1
  fi
  echo "ok    ${name} (${actual} parser attempt(s))"
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
import re
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
REAL_RUBY="$(command -v ruby)"
if [[ -z "${REAL_RUBY}" ]]; then
  echo "ruby is required to test YAML parser retries" >&2
  exit 1
fi

check_consumer_compat() {
  python3 - "$1" "$2" "$3" <<'PY'
import re
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
unreleased_start = changelog.find("## [Unreleased]")
if unreleased_start < 0:
    errors.append("CHANGELOG.md has no Unreleased section")
next_h2 = re.search(r"^## .+$", changelog[unreleased_start + 1 :], re.MULTILINE)
first_release = -1
if next_h2 is not None:
    first_release = unreleased_start + 1 + next_h2.start()
    release_heading = next_h2.group(0)
    if re.fullmatch(
        r"## \[(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\] - [0-9]{4}-[0-9]{2}-[0-9]{2}",
        release_heading,
    ) is None:
        errors.append("CHANGELOG Unreleased is not followed by a dated semver release")
claims = (
    "mixed Action migration",
    "literal `false`",
    "sandbox-command",
    "v3.0.1 to v3.0.2",
    "not measured",
)
claim_positions = []
for needle in claims:
    position = changelog.find(needle)
    claim_positions.append(position)
    if position < 0:
        errors.append(f"CHANGELOG history does not name {needle!r}")
if first_release < 0:
    errors.append("CHANGELOG.md has no numbered release history")
elif all(position >= 0 for position in claim_positions):
    active = [unreleased_start < position < first_release for position in claim_positions]
    if any(active) and not all(active):
        errors.append("CHANGELOG Action migration claims are split across active and released history")
    elif not any(active) and any(position < first_release for position in claim_positions):
        errors.append("CHANGELOG Action migration claims precede active and released history")
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

echo "== bounded YAML parser timeout retry =="
timeout_tree="${scratch}/yaml-timeout-tree"
prepare_yaml_timeout_tree "${timeout_tree}"

transient_state="${scratch}/transient-timeout-attempts"
expect_ok "one transient timeout is retried" \
  run_yaml_timeout_checker "${timeout_tree}" "transient-timeout" "${transient_state}"
transient_attempts="$(cat "${transient_state}")"
if (( transient_attempts < 2 )); then
  echo "FAIL: transient timeout made ${transient_attempts} parser attempt(s); expected at least 2" >&2
  exit 1
fi
echo "ok    transient timeout was retried"

persistent_state="${scratch}/persistent-timeout-attempts"
if run_yaml_timeout_checker "${timeout_tree}" "persistent-timeout" "${persistent_state}" \
  >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: persistent YAML parser timeout stayed green" >&2
  exit 1
fi
if ! grep -Fq -- "YAML parse timed out" "${scratch}/err"; then
  echo "FAIL: persistent timeout lost the fail-closed diagnostic:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
expect_attempts "persistent timeout retry bound" "${persistent_state}" 2

failure_state="${scratch}/parser-failure-attempts"
if run_yaml_timeout_checker "${timeout_tree}" "parser-failure" "${failure_state}" \
  >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: ordinary YAML parser failure stayed green" >&2
  exit 1
fi
if ! grep -Fq -- "YAML parse failed: synthetic parser failure" "${scratch}/err"; then
  echo "FAIL: ordinary parser failure lost its diagnostic:" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
expect_attempts "ordinary parser failures are not retried" "${failure_state}" 1

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

echo "== released CHANGELOG history remains authoritative =="
cp "${CHANGELOG}" "${scratch}/CHANGELOG-released.md"
python3 - "${scratch}/CHANGELOG-released.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = "## [Unreleased]\n\n### Changed"
released = "## [99.99.99] - 2099-12-31"
for _ in range(2):
    if text.count(needle) == 1:
        text = text.replace(
            needle,
            f"## [Unreleased]\n\n{released}\n\n### Changed",
            1,
        )
    else:
        unreleased = text.find("## [Unreleased]")
        first_release = text.find("\n## [", unreleased + 1)
        if unreleased < 0 or first_release < 0:
            raise SystemExit("expected active or already released CHANGELOG history")
path.write_text(text, encoding="utf-8")
PY
expect_ok "consumer-compat-released-history" check_consumer_compat \
  "${DEPENDABOT}" "${PINNED_ACTIONS}" "${scratch}/CHANGELOG-released.md"
for claim in \
  'mixed Action migration' \
  'literal `false`' \
  'sandbox-command' \
  'v3.0.1 to v3.0.2' \
  'not measured'; do
  cp "${scratch}/CHANGELOG-released.md" "${scratch}/CHANGELOG-split.md"
  python3 - "${scratch}/CHANGELOG-split.md" "${claim}" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
claim = sys.argv[2]
text = path.read_text(encoding="utf-8")
if text.count(claim) != 1:
    raise SystemExit(f"expected one claim occurrence: {claim}")
text = text.replace(claim, "", 1)
text = text.replace("## [Unreleased]\n", f"## [Unreleased]\n\n{claim}\n", 1)
path.write_text(text, encoding="utf-8")
PY
  if check_consumer_compat "${DEPENDABOT}" "${PINNED_ACTIONS}" \
    "${scratch}/CHANGELOG-split.md" >"${scratch}/out" 2>"${scratch}/err"; then
    echo "FAIL: split released claim stayed green: ${claim}" >&2
    exit 1
  fi
  if ! grep -Fq "split across active and released history" "${scratch}/err"; then
    echo "FAIL: split released claim did not name placement drift: ${claim}" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
done
echo "ok    released-history-placement (five owner-gate failures)"

cp "${CHANGELOG}" "${scratch}/CHANGELOG-preamble.md"
python3 - "${scratch}/CHANGELOG-preamble.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
claims = (
    "mixed Action migration",
    "literal `false`",
    "sandbox-command",
    "v3.0.1 to v3.0.2",
    "not measured",
)
for claim in claims:
    if text.count(claim) != 1:
        raise SystemExit(f"expected one claim occurrence: {claim}")
    text = text.replace(claim, "", 1)
text = "\n".join(claims) + "\n" + text
path.write_text(text, encoding="utf-8")
PY
if check_consumer_compat "${DEPENDABOT}" "${PINNED_ACTIONS}" \
  "${scratch}/CHANGELOG-preamble.md" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: coordinated preamble claims stayed green" >&2
  exit 1
fi
grep -Fq "precede active and released history" "${scratch}/err" \
  || { cat "${scratch}/err" >&2; exit 1; }
echo "ok    coordinated-preamble-placement (owner gate failed)"

cp "${CHANGELOG}" "${scratch}/CHANGELOG-nonversion.md"
python3 - "${scratch}/CHANGELOG-nonversion.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = "## [Unreleased]\n\n"
if text.count(needle) != 1:
    raise SystemExit("expected one Unreleased heading")
path.write_text(
    text.replace(needle, f"{needle}## [Migration Notes]\n\n", 1),
    encoding="utf-8",
)
PY
if check_consumer_compat "${DEPENDABOT}" "${PINNED_ACTIONS}" \
  "${scratch}/CHANGELOG-nonversion.md" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: non-version heading stayed green" >&2
  exit 1
fi
grep -Fq "not followed by a dated semver release" "${scratch}/err" \
  || { cat "${scratch}/err" >&2; exit 1; }
echo "ok    non-version-heading-placement (owner gate failed)"
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

# The three cases below cover the git branch of the walk. Every other negative case in this file
# runs against a scratch tree that is not a repository, so they all exercise the "scan everything"
# fallback. Without these, mutating the tracked filter into an unconditional skip leaves the whole
# battery green while the checker scans nothing on any real checkout.
echo "== git worktree: a tracked violation is still caught =="
copy_into "${scratch}/tracked-violation"
mkdir -p "${scratch}/tracked-violation/docs/getting-started"
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/tracked-violation/docs/getting-started/installation.md"
sgit -c init.defaultBranch=main -C "${scratch}/tracked-violation" init -q .
sgit -C "${scratch}/tracked-violation" add docs/getting-started/installation.md
expect_fail "tracked-violation-in-worktree" "is not on the owner snippet list" \
  "${scratch}/tracked-violation"

echo "== git worktree: an untracked file is not repository content =="
copy_into "${scratch}/untracked-violation"
mkdir -p "${scratch}/untracked-violation/docs/getting-started"
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/untracked-violation/docs/getting-started/installation.md"
sgit -c init.defaultBranch=main -C "${scratch}/untracked-violation" init -q .
sgit -C "${scratch}/untracked-violation" add .github/assay-action-pin
expect_ok "untracked-violation-in-worktree" run_checker_at "${scratch}/untracked-violation"

# The same tree, interrogated with a poisoned git environment. Scrubbed, git answers about THIS
# tree, the violating file is untracked and is ignored. Unscrubbed, git answers about the poisoned
# repository instead, its toplevel is not this tree, the checker falls back to scanning everything
# and the untracked violation turns it red -- so this case is what makes git_env() load-bearing
# rather than decorative. GIT_DIR and GIT_INDEX_FILE are set by pre-commit and by git hooks, so the
# poisoned shape here is the ordinary one, not a contrived one.
echo "== a poisoned git environment does not redirect the tracked-set question =="
mkdir -p "${scratch}/pin-poison"
sgit -c init.defaultBranch=main -C "${scratch}/pin-poison" init -q .
printf 'seed\n' >"${scratch}/pin-poison/seed.txt"
sgit -C "${scratch}/pin-poison" add seed.txt
poisoned_checker() {
  GIT_DIR="${scratch}/pin-poison/.git" \
  GIT_INDEX_FILE="${scratch}/pin-poison/.git/index" \
  GIT_WORK_TREE="${scratch}/pin-poison" \
    run_checker_at "${scratch}/untracked-violation"
}
expect_ok "poisoned-git-env-does-not-redirect-the-tracked-set" poisoned_checker

echo "== nested checkout: a copy at a path this tree does not own is pruned =="
copy_into "${scratch}/nested-checkout"
mkdir -p "${scratch}/nested-checkout/worktrees/inner/docs/getting-started"
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/nested-checkout/worktrees/inner/docs/getting-started/installation.md"
sgit -c init.defaultBranch=main -C "${scratch}/nested-checkout/worktrees/inner" init -q .
expect_ok "nested-checkout-pruned" run_checker_at "${scratch}/nested-checkout"

echo "== git worktree with nothing tracked falls back rather than trusting an empty listing =="
copy_into "${scratch}/empty-index"
mkdir -p "${scratch}/empty-index/docs/getting-started"
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/empty-index/docs/getting-started/installation.md"
sgit -c init.defaultBranch=main -C "${scratch}/empty-index" init -q .
expect_fail "empty-index-falls-back" "is not on the owner snippet list" "${scratch}/empty-index"

echo "== a tree inside a repo but not at its root falls back rather than trusting a partial listing =="
mkdir -p "${scratch}/inside-repo"
sgit -c init.defaultBranch=main -C "${scratch}/inside-repo" init -q .
copy_into "${scratch}/inside-repo/subtree"
mkdir -p "${scratch}/inside-repo/subtree/docs/getting-started"
sgit -C "${scratch}/inside-repo" add subtree/.github/assay-action-pin
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/inside-repo/subtree/docs/getting-started/installation.md"
expect_fail "subtree-of-repo-falls-back" "is not on the owner snippet list" \
  "${scratch}/inside-repo/subtree"

# The fallback must say why. Silence is how a check goes blind, and without this the note can be
# emptied out with no test noticing.
expect_fail "fallback-announces-itself" "is not a worktree root" \
  "${scratch}/inside-repo/subtree"

# A LINKED worktree's .git is a regular file, not a directory. Every case above builds its nested
# checkout with `git init`, whose .git is a directory, so `.exists()` could be weakened to
# `.is_dir()` and nothing would notice -- while the motivating case in this repository is exactly
# a linked worktree under .claude/worktrees/.
echo "== nested LINKED worktree, whose .git is a file, is pruned =="
copy_into "${scratch}/linked-worktree"
mkdir -p "${scratch}/linked-wt-origin"
sgit -c init.defaultBranch=main -C "${scratch}/linked-wt-origin" init -q .
printf 'seed\n' >"${scratch}/linked-wt-origin/seed.txt"
sgit -C "${scratch}/linked-wt-origin" add seed.txt
sgit -C "${scratch}/linked-wt-origin" -c user.email=t@e -c user.name=t commit -q -m seed
sgit -C "${scratch}/linked-wt-origin" worktree add -q "${scratch}/linked-worktree/inner" -b probe
mkdir -p "${scratch}/linked-worktree/inner/docs/getting-started"
printf '%s\n' '- uses: Rul1an/assay-action@v3' \
  >"${scratch}/linked-worktree/inner/docs/getting-started/installation.md"
test -f "${scratch}/linked-worktree/inner/.git" ||
  { echo "FAIL: linked worktree .git is not a regular file; case is vacuous" >&2; exit 1; }
expect_ok "linked-worktree-pruned" run_checker_at "${scratch}/linked-worktree"

echo "== unlisted tilde-fence YAML snippet is inventoried =="
copy_into "${scratch}/unlisted-tilde"
mkdir -p "${scratch}/unlisted-tilde/docs/getting-started"
cat >"${scratch}/unlisted-tilde/docs/getting-started/tilde-fence.md" <<'MD'
~~~yaml
steps:
  - uses: Rul1an/assay-action@v3
    with:
      evidence_mode: required
~~~
MD
expect_fail "unlisted-tilde-fence" "is not on the owner snippet list" "${scratch}/unlisted-tilde"

echo "== listed tilde-fence YAML snippets remain accepted =="
copy_into "${scratch}/listed-tilde"
python3 - "${scratch}/listed-tilde/docs/guides/github-action.md" <<'PY'
from pathlib import Path
import re
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
open_fence = re.compile(r"^[ \t]*```(ya?ml)[^\n`]*$", re.IGNORECASE | re.MULTILINE)
close_fence = re.compile(r"^[ \t]*```[ \t]*$", re.MULTILINE)
out = []
index = 0
converted = 0
while True:
    match = open_fence.search(text, index)
    if match is None:
        out.append(text[index:])
        break
    closer = close_fence.search(text, match.end())
    if closer is None:
        raise SystemExit("listed yaml fence is unclosed")
    out.append(text[index:match.start()])
    out.append(match.group(0).replace("```", "~~~", 1))
    out.append(text[match.end() : closer.start()])
    out.append(closer.group(0).replace("```", "~~~", 1))
    index = closer.end()
    converted += 1
if converted < 1:
    raise SystemExit("listed tilde conversion found no yaml fences")
path.write_text("".join(out), encoding="utf-8")
if "```yaml" in path.read_text(encoding="utf-8"):
    raise SystemExit("listed tilde conversion left a backtick yaml fence")
PY
expect_ok "listed-tilde-fence-accepted" run_checker_at "${scratch}/listed-tilde"

echo "== listed tilde-fence YAML snippet still checks with: inputs =="
copy_into "${scratch}/listed-tilde-input"
cat >"${scratch}/listed-tilde-input/docs/guides/github-action.md" <<'MD'
~~~yaml
steps:
  - uses: Rul1an/assay-action@v3
    with:
      evidence_mode: required
      undeclared_tilde_input: true
~~~
MD
expect_fail "listed-tilde-fence-undeclared-input" "undeclared input 'undeclared_tilde_input'" "${scratch}/listed-tilde-input"

echo "== unlisted tilde content line is not a closing fence =="
copy_into "${scratch}/unlisted-tilde-prefix"
mkdir -p "${scratch}/unlisted-tilde-prefix/docs/getting-started"
cat >"${scratch}/unlisted-tilde-prefix/docs/getting-started/prefix-close.md" <<'MD'
~~~yaml
~~~not-a-close
steps:
  - uses: Rul1an/assay-action@v3
    with:
      evidence_mode: required
~~~
MD
expect_fail "unlisted-tilde-prefix-not-close" "is not on the owner snippet list" "${scratch}/unlisted-tilde-prefix"

echo "== listed tilde longer closing run remains accepted =="
copy_into "${scratch}/listed-tilde-longer"
cat >"${scratch}/listed-tilde-longer/docs/guides/github-action.md" <<'MD'
~~~yaml
steps:
  - uses: Rul1an/assay-action@v3
    with:
      evidence_mode: required
~~~~
MD
expect_ok "listed-tilde-longer-close-accepted" run_checker_at "${scratch}/listed-tilde-longer"

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

echo "== unlisted oversize file without assay-action token =="
copy_into "${scratch}/oversize-notoken"
mkdir -p "${scratch}/oversize-notoken/docs/getting-started"
python3 - "${scratch}/oversize-notoken/docs/getting-started/notes.md" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
limit = 1048576
path.write_bytes(b"x" * (limit + 1))
if path.stat().st_size <= limit:
    raise SystemExit("oversize-notoken mutation did not exceed 1048576 bytes")
if b"assay-action" in path.read_bytes():
    raise SystemExit("oversize-notoken mutation contains assay-action")
PY
expect_fail "unlisted-oversize-without-token" "exceeds 1048576-byte limit" "${scratch}/oversize-notoken"

echo "== small irrelevant unlisted file remains allowed =="
copy_into "${scratch}/small"
mkdir -p "${scratch}/small/docs/getting-started"
printf '%s\n' 'irrelevant notes without a consumer action snippet' \
  >"${scratch}/small/docs/getting-started/notes.md"
expect_ok "small-irrelevant-unlisted-file" run_checker_at "${scratch}/small"

echo "== oversized generated vmlinux.rs remains allowed =="
copy_into "${scratch}/ebpf-vmlinux"
mkdir -p "${scratch}/ebpf-vmlinux/crates/assay-ebpf/src"
python3 - "${scratch}/ebpf-vmlinux/crates/assay-ebpf/src/vmlinux.rs" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
limit = 1048576
path.write_bytes(b"x" * (limit + 1))
if path.stat().st_size <= limit:
    raise SystemExit("generated vmlinux mutation did not exceed 1048576 bytes")
PY
expect_ok "generated-vmlinux-oversize-allowed" run_checker_at "${scratch}/ebpf-vmlinux"

echo "== assay-ebpf README action snippet is still inventoried =="
copy_into "${scratch}/ebpf-readme"
mkdir -p "${scratch}/ebpf-readme/crates/assay-ebpf"
printf '%s\n' '```yaml' '- uses: Rul1an/assay-action@v3' '```' \
  >"${scratch}/ebpf-readme/crates/assay-ebpf/README.md"
expect_fail "ebpf-readme-action-snippet" "is not on the owner snippet list" "${scratch}/ebpf-readme"

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

echo "== action discovery junction (#2778) =="
bash "${ROOT}/scripts/ci/test-action-discovery-junction.sh"

echo "== published remediation_recipe.cmd drift =="
copy_into "${scratch}/recipe-drift"
printf 'x' >>"${scratch}/recipe-drift/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd"
if ! run_checker_at "${scratch}/recipe-drift" >"${scratch}/out" 2>"${scratch}/err"; then
  echo "FAIL: offline pin check should stay green when only recipe drifts" >&2
  cat "${scratch}/err" >&2
  exit 1
fi
echo "ok    recipe-drift-offline-blind"
cp "${FIXTURE}" "${scratch}/recipe-pub-action.yml"
cp "${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd" "${scratch}/recipe-pub-oracle.cmd"
ASSAY_ACTION_PUBLISHED_FILE="${scratch}/recipe-pub-action.yml" \
  ASSAY_ACTION_PUBLISHED_RECIPE_FILE="${scratch}/recipe-pub-oracle.cmd" \
  expect_fail "published-recipe-drift" "does not match published recipe" "${scratch}/recipe-drift" --published


echo "assay action consumer pin contract: PASS"
