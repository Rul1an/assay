#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/ci/lib/drift-tree-snapshot.sh
source "$SCRIPT_DIR/lib/drift-tree-snapshot.sh"

ROOT="$(without_git_context git rev-parse --show-toplevel)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
SEED="$SCRATCH/seed"
SELECTED_CASE="${ASSAY_DOCS_DRIFT_SELF_TEST_CASE:-}"
INTERRUPT_CASE="${ASSAY_DOCS_DRIFT_INTERRUPT_AFTER_MUTATION:-}"
GATE_OUTPUT=""
# Full mode is a fixed mutation battery; selected mode deliberately executes one row.
EXPECTED_CASES=14

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
