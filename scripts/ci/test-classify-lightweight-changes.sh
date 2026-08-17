#!/usr/bin/env bash
# Prove the ordinary lightweight-only classifier and that ci.yml invokes it
# in both workflow_dispatch-with-PR and normal-event paths. Do not restate
# the changed-file rule here; call the real classifier.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CLASSIFIER="${ROOT}/scripts/ci/classify-lightweight-changes.sh"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

classify() {
  local list
  list="$(mktemp)"
  if (($#)); then
    printf '%s\n' "$@" >"$list"
  else
    : >"$list"
  fi
  bash "$CLASSIFIER" "$list"
  rm -f "$list"
}

expect_class() {
  local expected="$1"
  shift
  local got
  got="$(classify "$@")"
  [[ "$got" == "$expected" ]] || fail "classifier($*) => $got, expected $expected"
}

[[ -x "$CLASSIFIER" || -f "$CLASSIFIER" ]] || fail "missing $CLASSIFIER"

expect_class false
expect_class true "docs/foo.md"
expect_class false "packaging/agent-plugin/skills/assay-golden-path/SKILL.md"
expect_class false "docs/foo.md" "packaging/agent-plugin/skills/assay-golden-path/SKILL.md"
expect_class true "mkdocs.yml"
expect_class true "scripts/ci/review-example.sh"
expect_class false "crates/assay-cli/src/lib.rs"

python3 - "$WORKFLOW" <<'PY' || fail "ci.yml does not invoke the classifier in both required detect paths"
import re
import sys

text = open(sys.argv[1]).read()
start = text.find("      - id: detect\n")
if start < 0:
    raise SystemExit("could not find the detect step")
rest = text[start:]
run_at = rest.find("        run: |")
if run_at < 0:
    raise SystemExit("detect step has no run block")
body_start = start + run_at
lines = text[body_start:].splitlines()
indent = len(lines[0]) - len(lines[0].lstrip())
body = []
for line in lines[1:]:
    if line.strip() and (len(line) - len(line.lstrip())) <= indent:
        break
    body.append(line)
script = "\n".join(body)
if script.count("lightweight_only=true") != 0:
    raise SystemExit("detect step still hardcodes lightweight_only=true")
invocations = [
    line
    for line in body
    if "scripts/ci/classify-lightweight-changes.sh" in line
]
if len(invocations) != 2:
    raise SystemExit(
        f"detect step must call classify-lightweight-changes.sh twice, found {len(invocations)}"
    )
dispatch_pr = re.search(
    r'GITHUB_EVENT_NAME\}" == "workflow_dispatch" &&.*?elif \[\[ "\$\{GITHUB_EVENT_NAME\}" == "workflow_dispatch"',
    script,
    re.S,
)
bare_dispatch = re.search(
    r'elif \[\[ "\$\{GITHUB_EVENT_NAME\}" == "workflow_dispatch" \]\]; then(.*?)else',
    script,
    re.S,
)
normal = re.search(
    r'elif \[\[ "\$\{GITHUB_EVENT_NAME\}" == "workflow_dispatch" \]\]; then.*?else\n(.*)\Z',
    script,
    re.S,
)
if dispatch_pr is None or bare_dispatch is None or normal is None:
    raise SystemExit("could not split detect branches")
if "classify-lightweight-changes.sh" not in dispatch_pr.group(0):
    raise SystemExit("workflow_dispatch-with-PR path does not invoke the classifier")
if "classify-lightweight-changes.sh" in bare_dispatch.group(1):
    raise SystemExit("manual workflow_dispatch-without-PR must not invoke the classifier")
if "lightweight_only=false" not in bare_dispatch.group(1):
    raise SystemExit("manual workflow_dispatch-without-PR must stay lightweight_only=false")
if "classify-lightweight-changes.sh" not in normal.group(1):
    raise SystemExit("normal-event path does not invoke the classifier")
print("ci.yml detect paths invoke the classifier")
PY

echo "ok: lightweight classifier cases and ci.yml invocations"
