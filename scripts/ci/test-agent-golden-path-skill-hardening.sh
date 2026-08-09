#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# Task 1 pins the nine existing cases plus its two scheduling mutations.
EXPECTED_CASES=11
case_count=0

record_case_pass() {
  local name="$1"
  case_count=$((case_count + 1))
  echo "ok    $name"
}

seed_case() {
  local case_root="$1"
  mkdir -p \
    "$case_root/scripts/ci" \
    "$case_root/docs/generated" \
    "$case_root/docs/guides" \
    "$case_root/.agents/skills/assay-golden-path" \
    "$case_root/.claude/skills/assay-golden-path" \
    "$case_root/.github/workflows"
  cp "$ROOT/scripts/ci/test-agent-golden-path-skill.py" "$case_root/scripts/ci/"
  cp "$ROOT/.pre-commit-config.yaml" "$case_root/"
  cp "$ROOT/docs/generated/agent-golden-path.json" "$case_root/docs/generated/"
  cp "$ROOT/docs/guides/agent-golden-path.md" "$case_root/docs/guides/"
  cp "$ROOT/.agents/skills/assay-golden-path/SKILL.md" \
    "$case_root/.agents/skills/assay-golden-path/"
  cp "$ROOT/.claude/skills/assay-golden-path/SKILL.md" \
    "$case_root/.claude/skills/assay-golden-path/"
  cp "$ROOT/.github/workflows/kernel-matrix.yml" "$case_root/.github/workflows/"
}

expect_named_failure() {
  local name="$1" case_root="$2" expected="$3"
  local output="$case_root/validator.log"
  if python3 "$case_root/scripts/ci/test-agent-golden-path-skill.py" >"$output" 2>&1; then
    echo "FAIL: $name was accepted" >&2
    return 1
  fi
  if ! grep -Fq "$expected" "$output"; then
    cat "$output" >&2
    echo "FAIL: $name did not reach named guard: $expected" >&2
    return 1
  fi
  record_case_pass "$name"
}

case_root="$SCRATCH/guide-cwd"
seed_case "$case_root"
sed -i.bak 's/| 6\. Protected action | `examples\/privileged-action-gate` |/| 6. Protected action | `.` |/' \
  "$case_root/docs/guides/agent-golden-path.md"
rm "$case_root/docs/guides/agent-golden-path.md.bak"
expect_named_failure \
  "human guide without protected-action cwd" \
  "$case_root" \
  "guide omits working directory for protected-action"

for workflow_path in 'scripts/**' '.github/workflows/kernel-matrix.yml'; do
  for mutation in remove comment; do
    case_root="$SCRATCH/workflow-$mutation-$(printf '%s' "$workflow_path" | tr '/.*' '---')"
    seed_case "$case_root"
    WORKFLOW_PATH="$workflow_path" CASE_ROOT="$case_root" MUTATION="$mutation" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".github/workflows/kernel-matrix.yml"
line = f'      - "{os.environ["WORKFLOW_PATH"]}"\n'
text = path.read_text(encoding="utf-8")
if text.count(line) != 1:
    raise SystemExit(f"workflow path is not uniquely declared: {line!r}")
replacement = "" if os.environ["MUTATION"] == "remove" else f"      # {line.lstrip()}"
path.write_text(text.replace(line, replacement, 1), encoding="utf-8")
PY
    expect_named_failure \
      "workflow $mutation mutation for $workflow_path" \
      "$case_root" \
      "kernel-matrix workflow does not cover skill contract path: $workflow_path"
  done
done

for evidence in contract skill guide workflow; do
  case_root="$SCRATCH/oversized-$evidence"
  seed_case "$case_root"
  case "$evidence" in
    contract) path="$case_root/docs/generated/agent-golden-path.json" ;;
    skill) path="$case_root/.agents/skills/assay-golden-path/SKILL.md" ;;
    guide) path="$case_root/docs/guides/agent-golden-path.md" ;;
    workflow) path="$case_root/.github/workflows/kernel-matrix.yml" ;;
  esac
  python3 - "$path" <<'PY'
from pathlib import Path
import sys

with Path(sys.argv[1]).open("wb") as handle:
    handle.truncate(1048577)
PY
  expect_named_failure \
    "oversized $evidence evidence" \
    "$case_root" \
    "$evidence evidence exceeds 1048576-byte limit"
done

case_root="$SCRATCH/pre-push-only-drift-self-test"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".pre-commit-config.yaml"
source = (
    "        files: ^(scripts/ci/(check-docs-generated-drift|"
    "test-check-docs-generated-drift)\\.sh|scripts/docs/"
    "generate-agent-golden-path\\.py|\\.(agents|claude)/skills/"
    "assay-golden-path/SKILL\\.md)$\n"
)
replacement = "        stages: [pre-push]\n" + source
text = path.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"self-test files entry is not unique: {source!r}")
path.write_text(text.replace(source, replacement, 1), encoding="utf-8")
PY
expect_named_failure \
  "pre-push-only generated-docs drift self-test" \
  "$case_root" \
  "generated-docs drift self-test must run at the default pre-commit stage"

case_root="$SCRATCH/generator-outside-drift-self-test"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".pre-commit-config.yaml"
source = (
    "        files: ^(scripts/ci/(check-docs-generated-drift|"
    "test-check-docs-generated-drift)\\.sh|scripts/docs/"
    "generate-agent-golden-path\\.py|\\.(agents|claude)/skills/"
    "assay-golden-path/SKILL\\.md)$\n"
)
replacement = source.replace("scripts/docs/generate-agent-golden-path\\.py|", "")
text = path.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"self-test files entry is not unique: {source!r}")
path.write_text(text.replace(source, replacement, 1), encoding="utf-8")
PY
expect_named_failure \
  "golden-path generator outside generated-docs drift self-test" \
  "$case_root" \
  "generated-docs drift self-test does not cover its golden-path generator"

if (( case_count != EXPECTED_CASES )); then
  echo "FAIL: agent golden-path hardening expected $EXPECTED_CASES case(s), executed $case_count" >&2
  exit 1
fi

echo "agent golden-path hardening: $case_count case(s) executed"
