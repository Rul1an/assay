#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/ci/lib/drift-tree-snapshot.sh
source "$SCRIPT_DIR/lib/drift-tree-snapshot.sh"

ROOT="$(without_git_context git rev-parse --show-toplevel)"
CI_WORKFLOW="$ROOT/.github/workflows/ci.yml"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
SEED="$SCRATCH/seed"
SELECTED_CASE="${ASSAY_DOCS_DRIFT_SELF_TEST_CASE:-}"
INTERRUPT_CASE="${ASSAY_DOCS_DRIFT_INTERRUPT_AFTER_MUTATION:-}"
GATE_OUTPUT=""
# Full mode is a fixed mutation battery; selected mode deliberately executes one row.
EXPECTED_CASES=16

assert_generated_drift_job_contract() {
  python3 - "$CI_WORKFLOW" <<'PY'
from pathlib import Path
import re
import sys

def fail(message: str) -> None:
    raise AssertionError(message)

def job_section(workflow_text: str) -> str:
    job = re.search(
        r"(?ms)^  generated-drift:\n(.*?)(?=^  [A-Za-z0-9_-]+:|\Z)",
        workflow_text,
    )
    if not job:
        fail("ci.yml missing generated-drift job")
    return job.group(1)

def step_body(section: str, step_name: str) -> str:
    heading = f"      - name: {step_name}\n"
    at = section.find(heading)
    if at < 0:
        fail(f"generated-drift job missing step: {step_name}")
    rest = section[at + len(heading):]
    next_step = re.search(r"(?m)^      - ", rest)
    return rest if next_step is None else rest[:next_step.start()]

def active_commands(body: str, label: str) -> list[str]:
    run_at = body.find("        run: |\n")
    if run_at < 0:
        fail(f"{label} missing run block")
    script = body[run_at + len("        run: |\n"):]
    return [
        line.strip()
        for line in script.splitlines()
        if line.startswith("          ") and not line.lstrip().startswith("#")
    ]

def validate(workflow_text: str) -> None:
    section = job_section(workflow_text)
    if re.search(r"(?m)^    if:\s*", section):
        fail("generated-drift job must not set if:")
    if re.search(r"(?m)^    continue-on-error:\s*", section):
        fail("generated-drift job must not set continue-on-error:")

    verification_step = step_body(section, "Verify generated outputs are in sync")
    if re.search(r"(?m)^        (if|continue-on-error):", verification_step):
        fail("generated-drift verification step must not use if or continue-on-error")
    verification_commands = active_commands(
        verification_step,
        "generated-drift verification step",
    )
    required_verification = [
        "set -euo pipefail",
        "bash scripts/ci/check-docs-generated-drift.sh",
    ]
    if verification_commands != required_verification:
        fail(
            "generated-drift verification step must run exactly "
            f"{required_verification!r}, got {verification_commands!r}"
        )

    self_test_step = step_body(section, "Verify generated outputs self-test battery")
    if not re.search(
        r"(?m)^        if: needs\.scope\.outputs\.docs_generated_drift_inputs_touched == 'true'\s*$",
        self_test_step,
    ):
        fail(
            "generated-drift self-test step must be scoped by "
            "needs.scope.outputs.docs_generated_drift_inputs_touched == 'true'"
        )
    if re.search(r"(?m)^        continue-on-error:", self_test_step):
        fail("generated-drift self-test step must not use continue-on-error")
    self_test_commands = active_commands(self_test_step, "generated-drift self-test step")
    required_self_test = [
        "set -euo pipefail",
        "bash scripts/ci/test-check-docs-generated-drift-safety.sh",
    ]
    if self_test_commands != required_self_test:
        fail(
            "generated-drift self-test step must run exactly "
            f"{required_self_test!r}, got {self_test_commands!r}"
        )
    if any(
        command == "bash scripts/ci/test-check-docs-generated-drift.sh"
        for command in self_test_commands
    ):
        fail("generated-drift self-test battery must not run test-check-docs-generated-drift.sh twice")

workflow = Path(sys.argv[1]).read_text(encoding="utf-8")
try:
    validate(workflow)
except AssertionError as exc:
    raise SystemExit(f"FAIL: {exc}") from exc

mutant = workflow.replace(
    "      - name: Verify generated outputs are in sync\n",
    "      - name: Verify generated outputs are in sync\n"
    "        if: needs.scope.outputs.docs_generated_drift_inputs_touched == 'true'\n",
    1,
)
if mutant == workflow:
    raise SystemExit("FAIL: could not apply check-step if-mutation to generated-drift job")

try:
    validate(mutant)
except AssertionError as exc:
    if "generated-drift verification step must not use if or continue-on-error" not in str(exc):
        raise SystemExit(
            "FAIL: check-step if-mutation failed for the wrong reason: "
            f"{exc}"
        ) from exc
else:
    raise SystemExit("FAIL: generated-drift check-step if-mutation passed")
print("ok    generated-drift check-step if-mutation is rejected")
PY
}

seed_repo() {
  local destination="$1"
  mkdir -p "$destination"
  hermetic_git "$ROOT" ls-files -z | tar -cf - --null -T - \
    | (cd "$destination" && tar -xf -)
  hermetic_git "$destination" -c init.defaultBranch=main init -q
  hermetic_git "$destination" config user.name "Assay drift self-test"
  hermetic_git "$destination" config user.email "assay-drift-self-test@example.invalid"
  hermetic_git "$destination" add -f -- .
  hermetic_git "$destination" commit -qm "seed generated-docs drift case"
}

run_gate() {
  local case_root="$1"
  (cd "$case_root" && \
    without_git_context bash scripts/ci/check-docs-generated-drift.sh)
}

expect_gate_status() {
  local name="$1" case_root="$2" expected="$3"
  GATE_OUTPUT="$SCRATCH/gate-$name.log"
  local status
  if run_gate "$case_root" >"$GATE_OUTPUT" 2>&1; then
    status=0
  else
    status=$?
  fi
  if [[ "$status" -ne "$expected" ]]; then
    cat "$GATE_OUTPUT" >&2
    echo "FAIL: $name gate exit $status, wanted $expected" >&2
    return 1
  fi
  echo "ok    $name"
}

expect_gate_output() {
  local name="$1" expected="$2"
  if ! grep -Fq "$expected" "$GATE_OUTPUT"; then
    cat "$GATE_OUTPUT" >&2
    echo "FAIL: $name did not reach named gate diagnostic: $expected" >&2
    return 1
  fi
}

copy_case() {
  local name="$1" case_root="$SCRATCH/cases/$1"
  mkdir -p "$case_root"
  cp -a "$SEED/." "$case_root/"
  printf '%s\n' "$case_root"
}

maybe_interrupt_after_mutation() {
  local name="$1"
  if [[ -z "$INTERRUPT_CASE" ]]; then
    return 0
  fi
  if [[ -z "$SELECTED_CASE" || "$INTERRUPT_CASE" != "$SELECTED_CASE" ]]; then
    echo "FAIL: ASSAY_DOCS_DRIFT_INTERRUPT_AFTER_MUTATION must equal the selected case" >&2
    exit 1
  fi
  if [[ "$INTERRUPT_CASE" == "$name" ]]; then
    echo "test interruption: $name"
    exit 97
  fi
}

remove_generator_destination() {
  local case_root="$1" destination="$2"
  CASE_ROOT="$case_root" DESTINATION="$destination" python3 - <<'PY'
from pathlib import Path
import os

path = Path(os.environ["CASE_ROOT"]) / "scripts/docs/generate-agent-golden-path.py"
line = f'    ROOT / "{os.environ["DESTINATION"]}",\n'
text = path.read_text(encoding="utf-8")
if text.count(line) != 1:
    raise SystemExit(f"generator destination is not unique: {line!r}")
path.write_text(text.replace(line, "", 1), encoding="utf-8")
PY
}

case_tree_in_sync() {
  local case_root="$1" name="$2"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 0
}

case_hand_edited_diagram() {
  local case_root="$1" name="$2"
  printf '\n%%%% drift planted by the drift-check self-test\n' \
    >> "$case_root/docs/generated/crate-deps.mermaid"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

case_hand_edited_machine_contract() {
  local case_root="$1" name="$2"
  printf '\n' >> "$case_root/docs/generated/agent-golden-path.json"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

case_hand_edited_rendered_guide_table() {
  local case_root="$1" name="$2"
  sed 's/| 1\. Install check |/| 1. Drifted install check |/' \
    "$case_root/docs/guides/agent-golden-path.md" \
    > "$case_root/docs/guides/agent-golden-path.md.tmp"
  mv "$case_root/docs/guides/agent-golden-path.md.tmp" \
    "$case_root/docs/guides/agent-golden-path.md"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

case_hand_edited_codex_skill() {
  local case_root="$1" name="$2"
  printf '\n%%%% drift planted by the drift-check self-test\n' \
    >> "$case_root/.agents/skills/assay-golden-path/SKILL.md"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

case_hand_edited_plugin_skill() {
  local case_root="$1" name="$2"
  printf '\n%%%% drift planted by the drift-check self-test\n' \
    >> "$case_root/packaging/claude-plugin/skills/assay-golden-path/SKILL.md"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

case_hand_edited_agent_plugin_skill() {
  local case_root="$1" name="$2"
  printf '\n%%%% drift planted by the drift-check self-test\n' \
    >> "$case_root/packaging/agent-plugin/skills/assay-golden-path/SKILL.md"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

case_hand_edited_agent_plugin_contract() {
  local case_root="$1" name="$2"
  printf '\n' \
    >> "$case_root/packaging/agent-plugin/skills/assay-golden-path/references/agent-golden-path.json"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
}

# Semantic no-op byte drift: same JSON object, different whitespace. This is
# the F1 gap: a required-check-visible pin that the generator still owns the
# package-root manifests, not only the skill copies.
plant_json_whitespace_drift() {
  local path="$1"
  CASE_PATH="$path" python3 - <<'PY'
from pathlib import Path
import json
import os

path = Path(os.environ["CASE_PATH"])
data = json.loads(path.read_text(encoding="utf-8"))
path.write_text(json.dumps(data, indent=4) + "\n", encoding="utf-8")
PY
}

case_hand_edited_agent_plugin_mcp() {
  local case_root="$1" name="$2"
  local drifted="packaging/agent-plugin/mcp.json"
  plant_json_whitespace_drift "$case_root/$drifted"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
  expect_gate_output "$name" "$drifted"
}

case_hand_edited_agent_plugin_manifest() {
  local case_root="$1" name="$2"
  local drifted="packaging/agent-plugin/plugin.json"
  plant_json_whitespace_drift "$case_root/$drifted"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
  expect_gate_output "$name" "$drifted"
}

case_missing_codex_destination() {
  local case_root="$1" name="$2"
  local destination=".agents/skills/assay-golden-path/SKILL.md"
  remove_generator_destination "$case_root" "$destination"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
  expect_gate_output "$name" "error: the generators did not produce $destination."
}

case_missing_claude_destination() {
  local case_root="$1" name="$2"
  local destination=".claude/skills/assay-golden-path/SKILL.md"
  remove_generator_destination "$case_root" "$destination"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
  expect_gate_output "$name" "error: the generators did not produce $destination."
}

case_missing_plugin_destination() {
  local case_root="$1" name="$2"
  local destination="packaging/claude-plugin/skills/assay-golden-path/SKILL.md"
  remove_generator_destination "$case_root" "$destination"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
  expect_gate_output "$name" "error: the generators did not produce $destination."
}

case_generator_unable_to_run() {
  local case_root="$1" name="$2"
  printf '#!/usr/bin/env bash\nexit 98\n' \
    > "$case_root/scripts/docs/generate-module-map.sh"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 1
  expect_gate_output "$name" \
    "error: scripts/docs/generate-module-map.sh failed inside the scratch copy"
  expect_gate_output "$name" "This is a 'could not check', not a pass."
}

case_gate_reads_working_tree() {
  local case_root="$1" name="$2"
  cat >> "$case_root/scripts/docs/generate-crate-deps.sh" <<'SH'
echo '    %% working-tree-only generator marker' >> "$OUTPUT_FILE"
SH
  (cd "$case_root" && bash scripts/docs/generate-crate-deps.sh >/dev/null)
  (cd "$case_root" && bash scripts/docs/update-architecture-docs.sh >/dev/null)
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 0
}

case_gate_leaves_worktree_untouched() {
  local case_root="$1" name="$2"
  local before after
  before="$(snapshot_tree "$case_root")"
  maybe_interrupt_after_mutation "$name"
  expect_gate_status "$name" "$case_root" 0
  after="$(snapshot_tree "$case_root")"
  if [[ "$before" != "$after" ]]; then
    echo "FAIL: $name rewrote the repository it audited" >&2
    diff -u <(printf '%s\n' "$before") <(printf '%s\n' "$after") >&2 || true
    return 1
  fi
}

CASES=(
  "tree-in-sync|case_tree_in_sync"
  "hand-edited-diagram|case_hand_edited_diagram"
  "hand-edited-machine-contract|case_hand_edited_machine_contract"
  "hand-edited-rendered-guide-table|case_hand_edited_rendered_guide_table"
  "hand-edited-codex-skill|case_hand_edited_codex_skill"
  "hand-edited-plugin-skill|case_hand_edited_plugin_skill"
  "hand-edited-agent-plugin-skill|case_hand_edited_agent_plugin_skill"
  "hand-edited-agent-plugin-contract|case_hand_edited_agent_plugin_contract"
  "hand-edited-agent-plugin-mcp|case_hand_edited_agent_plugin_mcp"
  "hand-edited-agent-plugin-manifest|case_hand_edited_agent_plugin_manifest"
  "missing-codex-skill-destination|case_missing_codex_destination"
  "missing-claude-skill-destination|case_missing_claude_destination"
  "missing-plugin-skill-destination|case_missing_plugin_destination"
  "generator-unable-to-run|case_generator_unable_to_run"
  "gate-reads-working-tree|case_gate_reads_working_tree"
  "gate-leaves-worktree-untouched|case_gate_leaves_worktree_untouched"
)

if [[ -n "$SELECTED_CASE" ]]; then
  selected_known=false
  for row in "${CASES[@]}"; do
    IFS='|' read -r name _ <<<"$row"
    if [[ "$name" == "$SELECTED_CASE" ]]; then
      selected_known=true
      break
    fi
  done
  if [[ "$selected_known" != true ]]; then
    echo "FAIL: unknown generated-docs drift self-test case: $SELECTED_CASE" >&2
    exit 1
  fi
fi

seed_repo "$SEED"
assert_generated_drift_job_contract
ROOT_BEFORE="$(snapshot_tree "$ROOT")"
executed_cases=0

for row in "${CASES[@]}"; do
  IFS='|' read -r name handler <<<"$row"
  if [[ -n "$SELECTED_CASE" && "$name" != "$SELECTED_CASE" ]]; then
    continue
  fi
  echo "running drift case: $name"
  case_root="$(copy_case "$name")"
  "$handler" "$case_root" "$name"
  executed_cases=$((executed_cases + 1))
done

if [[ -n "$SELECTED_CASE" && "$executed_cases" -ne 1 ]]; then
  echo "FAIL: selected drift self-test executed $executed_cases cases, wanted 1" >&2
  exit 1
fi
if [[ -z "$SELECTED_CASE" && "$executed_cases" -ne "$EXPECTED_CASES" ]]; then
  echo "FAIL: full drift self-test executed $executed_cases cases, wanted $EXPECTED_CASES" >&2
  exit 1
fi
echo "generated-docs drift self-test: $executed_cases case(s) executed"

ROOT_AFTER="$(snapshot_tree "$ROOT")"
if [[ "$ROOT_BEFORE" != "$ROOT_AFTER" ]]; then
  echo "FAIL: generated-docs self-test changed the reviewable repository tree" >&2
  diff -u <(printf '%s\n' "$ROOT_BEFORE") <(printf '%s\n' "$ROOT_AFTER") >&2 || true
  exit 1
fi

if [[ -z "$SELECTED_CASE" ]]; then
  meta_root="$(copy_case snapshot-meta-mutation)"
  meta_before="$(snapshot_tree "$meta_root")"
  printf '\n%%%% snapshot meta-mutation\n' \
    >> "$meta_root/docs/generated/crate-deps.mermaid"
  meta_after="$(snapshot_tree "$meta_root")"
  if [[ "$meta_before" == "$meta_after" ]]; then
    echo "FAIL: repository snapshot ignored a generated-docs mutation" >&2
    exit 1
  fi
  meta_diff="$SCRATCH/snapshot-meta-mutation.diff"
  diff -u \
    <(printf '%s\n' "$meta_before") \
    <(printf '%s\n' "$meta_after") >"$meta_diff" || true
  if ! grep -Fq 'docs/generated/crate-deps.mermaid' "$meta_diff"; then
    cat "$meta_diff" >&2
    echo "FAIL: repository snapshot diff did not name docs/generated/crate-deps.mermaid" >&2
    exit 1
  fi
  echo "ok    repository snapshot detects its generated-docs meta-mutation"
fi
