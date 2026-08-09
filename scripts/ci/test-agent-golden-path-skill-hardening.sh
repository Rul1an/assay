#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# This cumulative pin keeps Task 1's 11 cases while Task 2 adds 12 parser
# rejections and one parser allow probe, so later tasks cannot silently drop either.
EXPECTED_CASES=24
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

mutate_workflow() {
  local case_root="$1" mutation="$2"
  CASE_ROOT="$case_root" MUTATION="$mutation" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".github/workflows/kernel-matrix.yml"
text = path.read_text(encoding="utf-8")
mutation = os.environ["MUTATION"]

replacements = {
    "tab-path": (
        '      - "crates/assay-ebpf/**"',
        '\t      - "crates/assay-ebpf/**"',
    ),
    "duplicate-paths": (
        '    paths:\n',
        '    paths:\n    paths:\n',
    ),
    "delete-paths": (
        '    paths:\n',
        '',
    ),
    "unquoted-path": (
        '      - "crates/assay-ebpf/**"',
        '      - crates/assay-ebpf/**',
    ),
    "paths-ignore": (
        '    paths:\n',
        '    paths-ignore:\n      - "docs/**"\n    paths:\n',
    ),
    "branches-ignore": (
        '    branches: [ "main" ]\n',
        '    branches: [ "main" ]\n    branches-ignore: [ "release/**" ]\n',
    ),
    "release-branch": (
        '    branches: [ "main" ]',
        '    branches: [ "release/*" ]',
    ),
    "unquoted-branch": (
        '    branches: [ "main" ]',
        '    branches: [main]',
    ),
    "block-branch": (
        '    branches: [ "main" ]',
        '    branches:\n      - "main"',
    ),
    "types": (
        '    paths:\n',
        '    types: ["labeled"]\n    paths:\n',
    ),
    "comment-pull-request": (
        '  pull_request:\n',
        '  # pull_request:\n',
    ),
    "delete-lint-runs-on": (
        '  lint:\n    name: Lint (pre-commit)\n    runs-on: ubuntu-latest\n',
        '  lint:\n    name: Lint (pre-commit)\n',
    ),
}

try:
    source, replacement = replacements[mutation]
except KeyError as error:
    raise SystemExit(f"unknown workflow mutation: {mutation}") from error
if text.count(source) != 1:
    raise SystemExit(f"workflow mutation anchor is not unique: {source!r}")
path.write_text(text.replace(source, replacement, 1), encoding="utf-8")
PY
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

declare -a parser_mutations=(
  "tab-path|tab before pull-request path|kernel-matrix workflow uses tab indentation"
  "duplicate-paths|duplicate pull-request paths key|kernel-matrix pull_request duplicates key: paths"
  "delete-paths|missing pull-request paths key|kernel-matrix pull_request is missing required key: paths"
  "unquoted-path|unquoted pull-request path|kernel-matrix pull_request.paths contains an unsupported entry"
  "paths-ignore|pull-request paths-ignore|kernel-matrix pull_request cannot combine paths and paths-ignore"
  "branches-ignore|pull-request branches-ignore|kernel-matrix pull_request cannot combine branches and branches-ignore"
  "release-branch|pull-request release-only branch|kernel-matrix pull_request does not cover main"
  "unquoted-branch|unquoted pull-request branch|kernel-matrix pull_request.branches must be a bracketed list of quoted strings"
  "block-branch|block pull-request branch|kernel-matrix pull_request.branches must be a bracketed list of quoted strings"
  "types|pull-request types|kernel-matrix pull_request must not declare types"
  "comment-pull-request|commented pull-request section|kernel-matrix workflow must declare exactly one pull_request section"
  "delete-lint-runs-on|missing lint runner|kernel-matrix lint job is missing required key: runs-on"
)

for parser_case in "${parser_mutations[@]}"; do
  IFS='|' read -r mutation name expected <<<"$parser_case"
  case_root="$SCRATCH/parser-$mutation"
  seed_case "$case_root"
  mutate_workflow "$case_root" "$mutation"
  expect_named_failure "$name" "$case_root" "$expected"
done

case_root="$SCRATCH/inline-run-parser"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".github/workflows/kernel-matrix.yml"
source = "run: cargo install --locked cargo-deny"
replacement = "run: echo inline-parser-sentinel"
text = path.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"inline run anchor is not unique: {source!r}")
path.write_text(text.replace(source, replacement, 1), encoding="utf-8")
PY
if ! CASE_ROOT="$case_root" python3 - <<'PY'
import importlib.util
import os
from pathlib import Path
import sys

case_root = Path(os.environ["CASE_ROOT"])
validator_path = case_root / "scripts/ci/test-agent-golden-path-skill.py"
spec = importlib.util.spec_from_file_location("golden_path_validator", validator_path)
if spec is None or spec.loader is None:
    raise SystemExit(f"cannot import copied validator: {validator_path}")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
workflow = (case_root / ".github/workflows/kernel-matrix.yml").read_text(encoding="utf-8")
contract = module.parse_kernel_matrix_workflow(workflow)
if sum(step.shell_lines == ("echo inline-parser-sentinel",) for step in contract.lint_steps) != 1:
    raise SystemExit("parser did not retain exactly one inline run sentinel")

uses_only_indices = []
lines = workflow.splitlines()
lint_start = lines.index("  lint:")
lint_end = next(
    (
        index
        for index, line in enumerate(lines[lint_start + 1 :], start=lint_start + 1)
        if line.startswith("  ") and not line.startswith("    ")
    ),
    len(lines),
)
lint_lines = lines[lint_start + 1 : lint_end]
for index, line in enumerate(lint_lines):
    if not line.startswith("      - uses:"):
        continue
    step_lines = []
    for candidate in lint_lines[index + 1 :]:
        if candidate.startswith("      - "):
            break
        step_lines.append(candidate)
    if not any(candidate.lstrip().startswith("run:") for candidate in step_lines):
        uses_only_indices.append(
            sum(1 for prior in lint_lines[:index] if prior.startswith("      - "))
        )

if not uses_only_indices:
    raise SystemExit("scratch workflow did not retain uses-only steps")
for step_index in uses_only_indices:
    if contract.lint_steps[step_index].shell_lines:
        raise SystemExit("uses-only steps must have empty shell lines")
PY
then
  echo "FAIL: inline run parser probe failed" >&2
  exit 1
fi
record_case_pass "inline run parser probe"

if (( case_count != EXPECTED_CASES )); then
  echo "FAIL: agent golden-path hardening expected $EXPECTED_CASES case(s), executed $case_count" >&2
  exit 1
fi

echo "agent golden-path hardening: $case_count case(s) executed"
