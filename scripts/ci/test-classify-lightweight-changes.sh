#!/usr/bin/env bash
# Prove the ordinary lightweight-only classifier and that ci.yml invokes it
# in both workflow_dispatch-with-PR and normal-event paths. Do not restate
# the changed-file rule here; call the real classifier.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CLASSIFIER="${ROOT}/scripts/ci/classify-lightweight-changes.sh"
DRIFT_SELF_TEST_SCOPE="${ROOT}/scripts/ci/docs-generated-drift-self-test-scope.py"
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
# Active docs and every Markdown file outside docs/archive/ take the full lane: Rust tests read
# and walk them (docs_corpus_loads, config_reference_examples_parse, documented_trace_events_read,
# include_str! readers in integration tests and in assay-cli's binary target).
expect_class false "docs/foo.md"
expect_class false "docs/guides/runtime-monitor.md"
expect_class false "docs/architecture/SPEC-PR-Gate-Outputs-v1.md"
expect_class false "docs/reference/receipt-schemas/receipts/promptfoo.assertion-component.v1.schema.json"
expect_class false "README.md"
expect_class false "CHANGELOG.md"
expect_class false "examples/crewai-event-evidence/README.md"
# docs/archive/ is history that no Rust test reads, so it stays lightweight; the prefix is a path
# component, not a string prefix.
expect_class true "docs/archive/CI-ASSESSMENT-TODO-legacy.md"
expect_class false "docs/archiveX/foo.md"
expect_class false "docs/archive/old.md" "docs/guides/runtime-monitor.md"
expect_class true "docs/archive/old.md" "mkdocs.yml"
# crates/ is never lightweight, Markdown included (exit-code docs compiled in with include_str!).
expect_class false "crates/assay-cli/README.md"
expect_class false "crates/assay-cli/src/exit_codes/evidence_integrity_boundary.md"
expect_class false "packaging/agent-plugin/skills/assay-golden-path/SKILL.md"
expect_class false "docs/archive/old.md" "packaging/agent-plugin/skills/assay-golden-path/SKILL.md"
expect_class true "mkdocs.yml"
expect_class true "scripts/ci/review-example.sh"
expect_class true ".github/ISSUE_TEMPLATE/bug_report.yml"
expect_class true ".github/DISCUSSION_TEMPLATE/ideas.yml"
expect_class false "crates/assay-cli/src/lib.rs"

# --- Renames: the list the detect step builds carries both the source and the destination -------
# git diff detects renames by default and then prints only the destination, so moving an active
# guide into docs/archive/ looked like an archive-only change. Run the detect step's own commands,
# read from the parsed workflow, on a synthetic repository, and its files-API jq on a synthetic
# response, and classify what they produce.
rename_regression() {
  # shellcheck source=scripts/ci/lib/clear-git-repository-env.sh
  # shellcheck disable=SC1091
  source "${ROOT}/scripts/ci/lib/clear-git-repository-env.sh"
  export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  local repo cmds jq_expr cmd list n=0 base head control
  repo="$(mktemp -d)"
  git -C "$repo" init -q
  git -C "$repo" config user.email ci@example.com
  git -C "$repo" config user.name CI
  mkdir -p "$repo/docs/guides" "$repo/docs/archive"
  printf 'guide\n' >"$repo/docs/guides/example.md"
  printf 'old\n' >"$repo/docs/archive/a.md"
  git -C "$repo" add -A && git -C "$repo" commit -q -m base
  base="$(git -C "$repo" rev-parse HEAD)"
  git -C "$repo" mv docs/guides/example.md docs/archive/example.md
  git -C "$repo" commit -q -m "archive the guide"
  head="$(git -C "$repo" rev-parse HEAD)"
  git -C "$repo" mv docs/archive/a.md docs/archive/b.md
  git -C "$repo" commit -q -m "rename inside the archive"
  control="$(git -C "$repo" rev-parse HEAD)"

  cmds="$(ruby -ryaml -e '
    d = YAML.safe_load_file(ARGV[0], aliases: false)
    step = d["jobs"]["scope"]["steps"].find { |s| s["id"] == "detect" }
    puts step["run"].lines.map(&:strip).grep(/\Agit diff(-tree)? /)' "$WORKFLOW")"
  [[ "$(grep -c . <<<"$cmds")" -eq 3 ]] || fail "expected three git diff commands in the detect step, found: $cmds"
  while IFS= read -r cmd; do
    list="$(mktemp)"
    (cd "$repo" && base_sha="$base" head_sha="$head" changed_file="$list" bash -c "set -euo pipefail; $cmd")
    grep -qx 'docs/guides/example.md' "$list" || fail "detect command '$cmd' omits the source of a rename"
    [[ "$(bash "$CLASSIFIER" "$list")" == false ]] || fail "detect command '$cmd': an active guide moved into docs/archive/ classified lightweight"
    (cd "$repo" && base_sha="$head" head_sha="$control" changed_file="$list" bash -c "set -euo pipefail; $cmd")
    [[ "$(bash "$CLASSIFIER" "$list")" == true ]] || fail "detect command '$cmd': a rename inside docs/archive/ should stay lightweight"
    rm -f "$list"
    n=$((n + 1))
  done <<<"$cmds"
  rm -rf "$repo"

  jq_expr="$(ruby -ryaml -e '
    d = YAML.safe_load_file(ARGV[0], aliases: false)
    run = d["jobs"]["scope"]["steps"].find { |s| s["id"] == "detect" }["run"]
    m = run.match(%r{pulls/\$\{DISPATCH_PR_NUMBER\}/files[^\n]*\n\s*--jq \x27([^\x27]*)\x27})
    puts m ? m[1] : ""' "$WORKFLOW")"
  [[ -n "$jq_expr" ]] || fail "could not read the files-API jq expression of the dispatch path"
  list="$(mktemp)"
  jq -r "$jq_expr" <<<'[{"filename":"docs/archive/example.md","previous_filename":"docs/guides/example.md","status":"renamed"}]' >"$list"
  grep -qx 'docs/guides/example.md' "$list" || fail "dispatch jq '$jq_expr' omits previous_filename"
  [[ "$(bash "$CLASSIFIER" "$list")" == false ]] || fail "dispatch path: an active guide moved into docs/archive/ classified lightweight"
  rm -f "$list"
  echo "renames: ${n} detect commands and the dispatch jq keep both paths"
}
( rename_regression )

scope_flag() {
  local list
  list="$(mktemp)"
  if (($#)); then
    printf '%s\n' "$@" >"$list"
  else
    : >"$list"
  fi
  python3 "$DRIFT_SELF_TEST_SCOPE" < "$list"
  rm -f "$list"
}

expect_scope_flag() {
  local expected="$1"
  shift
  local got
  got="$(scope_flag "$@")"
  [[ "$got" == "$expected" ]] || fail "scope-flag($*) => $got, expected $expected"
}

expect_scope_flag true "scripts/ci/check-docs-generated-drift.sh"
expect_scope_flag false "crates/assay-core/src/lib.rs"

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
if script.count("docs_generated_drift_inputs_touched=true") != 0:
    raise SystemExit("detect step still hardcodes docs_generated_drift_inputs_touched=true")
invocations = [
    line
    for line in body
    if "scripts/ci/classify-lightweight-changes.sh" in line
]
if len(invocations) != 2:
    raise SystemExit(
        f"detect step must call classify-lightweight-changes.sh twice, found {len(invocations)}"
    )
scope_invocations = [
    line
    for line in body
    if "scripts/ci/docs-generated-drift-self-test-scope.py" in line
]
if len(scope_invocations) != 1:
    raise SystemExit(
        "detect step must call docs-generated-drift-self-test-scope.py once, "
        f"found {len(scope_invocations)}"
    )
if not any("docs_generated_drift_inputs_touched=" in line for line in body):
    raise SystemExit("detect step does not assign docs_generated_drift_inputs_touched")
if 'echo "docs_generated_drift_inputs_touched=${docs_generated_drift_inputs_touched}"' not in script:
    raise SystemExit("detect step does not publish docs_generated_drift_inputs_touched")
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

echo "ok: lightweight classifier cases, drift self-test scope flag, and ci.yml invocations"
