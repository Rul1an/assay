#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

# Task 5 adds three tracked-symlink mutations to Task 4's 46-case boundary.
EXPECTED_CASES=49
# Parser-layer follow-ups stay outside the approved cumulative case chain. The
# 22 probes are 4 workflow-key, 1 stages-key, 14 selector, 1 trigger-mode, and
# 2 inline-parser checks; pin them so deletion cannot leave the 49-case total green.
EXPECTED_STRUCTURAL_PROBES=22
case_count=0
structural_probe_count=0

record_case_pass() {
  local name="$1"
  case_count=$((case_count + 1))
  echo "ok    $name"
}

record_case_skip() {
  local name="$1" diagnostic="$2"
  case_count=$((case_count + 1))
  echo "skip  $name: $diagnostic"
}

record_structural_probe_pass() {
  local name="$1"
  structural_probe_count=$((structural_probe_count + 1))
  echo "ok    structural probe: $name"
}

create_required_symlink() {
  local name="$1" target="$2" link="$3" output os_name
  if output="$(ln -s "$target" "$link" 2>&1)"; then
    return 0
  fi

  os_name="$(uname -s)"
  if [[ "$os_name" == "Linux" ]]; then
    echo "FAIL: $name could not create required symlink on Linux: $output" >&2
    exit 1
  fi
  record_case_skip "$name" "symlink creation refused on $os_name: $output"
  return 1
}

check_ast_source_scope() {
  HARDENING_PATH="$0" python3 - \
    "$ROOT/scripts/ci/test-agent-golden-path-skill.py" \
    "$ROOT/scripts/docs/generate-agent-golden-path.py" <<'PY'
import ast
import os
from pathlib import Path
import sys

if len(sys.argv) != 3:
    raise SystemExit("AST source check must scan exactly two Python source arguments")

for raw in sys.argv[1:]:
    path = Path(raw)
    ast.parse(path.read_text(encoding="utf-8"), filename=str(path))

# Bash heredoc Python remains an explicit residual: this source-file-only AST
# check does not claim that embedded fragments are parsed or assertion-free.
hardening = Path(os.environ["HARDENING_PATH"]).read_text(encoding="utf-8")
if "python3 - <<'PY'" not in hardening:
    raise SystemExit("AST source check must retain its embedded-Python residual")
PY
}

check_ast_source_scope

case_git() {
  local case_root="$1"
  shift
  env \
    -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
    -u GIT_COMMON_DIR \
    -u GIT_DIR \
    -u GIT_INDEX_FILE \
    -u GIT_OBJECT_DIRECTORY \
    -u GIT_WORK_TREE \
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    git -C "$case_root" "$@"
}

seed_case() {
  local case_root="$1"
  mkdir -p \
    "$case_root/scripts/ci" \
    "$case_root/scripts/docs" \
    "$case_root/docs/generated" \
    "$case_root/docs/guides" \
    "$case_root/.agents/skills/assay-golden-path" \
    "$case_root/.claude/skills/assay-golden-path" \
    "$case_root/.github/workflows"
  cp "$ROOT/scripts/ci/test-agent-golden-path-skill.py" "$case_root/scripts/ci/"
  cp "$ROOT/scripts/docs/generate-agent-golden-path.py" "$case_root/scripts/docs/"
  cp "$ROOT/.gitignore" "$case_root/"
  cp "$ROOT/.gitattributes" "$case_root/"
  cp "$ROOT/.pre-commit-config.yaml" "$case_root/"
  cp "$ROOT/docs/generated/agent-golden-path.json" "$case_root/docs/generated/"
  cp "$ROOT/docs/guides/agent-golden-path.md" "$case_root/docs/guides/"
  cp "$ROOT/.agents/skills/assay-golden-path/SKILL.md" \
    "$case_root/.agents/skills/assay-golden-path/"
  cp "$ROOT/.claude/skills/assay-golden-path/SKILL.md" \
    "$case_root/.claude/skills/assay-golden-path/"
  cp "$ROOT/.github/workflows/kernel-matrix.yml" "$case_root/.github/workflows/"
  case_git "$case_root" -c init.defaultBranch=main init -q
  case_git "$case_root" -c core.excludesFile= -c core.attributesFile= \
    add -f -- .
}

expect_failure_with_recorder() {
  local recorder="$1" name="$2" case_root="$3" expected="$4"
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
  "$recorder" "$name"
}

expect_named_failure() {
  expect_failure_with_recorder record_case_pass "$@"
}

expect_structural_failure() {
  expect_failure_with_recorder record_structural_probe_pass "$@"
}

expect_named_success() {
  local name="$1" case_root="$2"
  local output="$case_root/validator.log"
  if ! python3 "$case_root/scripts/ci/test-agent-golden-path-skill.py" >"$output" 2>&1; then
    cat "$output" >&2
    echo "FAIL: $name was rejected" >&2
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
    "duplicate-path-entry": (
        '      - "crates/assay-ebpf/**"\n',
        '      - "crates/assay-ebpf/**"\n      - "crates/assay-ebpf/**"\n',
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
    "spaced-types": (
        '    paths:\n',
        '    types : ["labeled"]\n    paths:\n',
    ),
    "comment-pull-request": (
        '  pull_request:\n',
        '  # pull_request:\n',
    ),
    "delete-lint-runs-on": (
        '  lint:\n    name: Lint (pre-commit)\n    runs-on: ubuntu-latest\n',
        '  lint:\n    name: Lint (pre-commit)\n',
    ),
    "comment-lint": (
        '  lint:\n',
        '  # lint:\n',
    ),
    "delete-canonical-pre-commit": (
        '          pre-commit run --all-files --show-diff-on-failure\n',
        '',
    ),
    "pre-commit-files": (
        '          pre-commit run --all-files --show-diff-on-failure\n',
        '          pre-commit run --all-files --show-diff-on-failure\n\n'
        '      - name: Run noncanonical pre-commit\n'
        '        shell: bash\n'
        '        run: pre-commit run --files --show-diff-on-failure\n',
    ),
    "pre-commit-hook-stage": (
        '          pre-commit run --all-files --show-diff-on-failure\n',
        '          pre-commit run --all-files --show-diff-on-failure --hook-stage pre-push\n',
    ),
    "lint-step-condition": (
        '      - name: Run pre-commit (all files)\n        shell: bash\n',
        '      - name: Run pre-commit (all files)\n        shell: bash\n        if: false\n',
    ),
    "lint-step-spaced-condition": (
        '      - name: Run pre-commit (all files)\n        shell: bash\n',
        '      - name: Run pre-commit (all files)\n        shell: bash\n        if : false\n',
    ),
    "lint-step-continue-on-error": (
        '      - name: Run pre-commit (all files)\n        shell: bash\n',
        '      - name: Run pre-commit (all files)\n        shell: bash\n        continue-on-error: true\n',
    ),
    "lint-step-spaced-continue-on-error": (
        '      - name: Run pre-commit (all files)\n        shell: bash\n',
        '      - name: Run pre-commit (all files)\n        shell: bash\n        continue-on-error : true\n',
    ),
    "lint-needs": (
        '  lint:\n    name: Lint (pre-commit)\n',
        '  lint:\n    name: Lint (pre-commit)\n    needs: optional-job\n',
    ),
    "lint-runner": (
        '  lint:\n    name: Lint (pre-commit)\n    runs-on: ubuntu-latest\n',
        '  lint:\n    name: Lint (pre-commit)\n    runs-on: ubuntu-24.04\n',
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

mutate_precommit_selector() {
  local case_root="$1" mutation="$2"
  CASE_ROOT="$case_root" MUTATION="$mutation" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".pre-commit-config.yaml"
text = path.read_text(encoding="utf-8")
mutation = os.environ["MUTATION"]
hook_files = (
    "        files: ^(scripts/ci/(check-docs-generated-drift|"
    "test-check-docs-generated-drift(?:-safety)?|lib/drift-tree-snapshot)\\.sh|scripts/docs/"
    "generate-agent-golden-path\\.py|\\.(agents|claude)/skills/"
    "assay-golden-path/SKILL\\.md)$\n"
)
root_anchor = "default_install_hook_types: [pre-commit, pre-push]\n"

replacements = {
    "spaced-stages": (
        hook_files,
        "        stages : [pre-push]\n" + hook_files,
    ),
    "root-files": (
        root_anchor,
        root_anchor + "files: ^does-not-match$\n",
    ),
    "root-spaced-files": (
        root_anchor,
        root_anchor + "files : ^does-not-match$\n",
    ),
    "root-exclude": (
        root_anchor,
        root_anchor + "exclude: .*\n",
    ),
    "root-spaced-exclude": (
        root_anchor,
        root_anchor + "exclude : .*\n",
    ),
    "hook-files": (
        hook_files,
        "        files: ^does-not-match$\n",
    ),
    "hook-spaced-files": (
        hook_files,
        "        files : ^does-not-match$\n",
    ),
    "hook-exclude": (
        hook_files,
        "        exclude: .*\n" + hook_files,
    ),
    "hook-exclude-inline-comment": (
        hook_files,
        "        exclude: .* # skip every path\n" + hook_files,
    ),
    "hook-spaced-exclude": (
        hook_files,
        "        exclude : .*\n" + hook_files,
    ),
    "hook-types": (
        hook_files,
        "        types: [image]\n" + hook_files,
    ),
    "hook-spaced-types": (
        hook_files,
        "        types : [image]\n" + hook_files,
    ),
    "hook-types-or": (
        hook_files,
        "        types_or: [image]\n" + hook_files,
    ),
    "hook-spaced-types-or": (
        hook_files,
        "        types_or : [image]\n" + hook_files,
    ),
    "hook-exclude-types": (
        hook_files,
        "        exclude_types: [file]\n" + hook_files,
    ),
    "hook-spaced-exclude-types": (
        hook_files,
        "        exclude_types : [file]\n" + hook_files,
    ),
}

try:
    source, replacement = replacements[mutation]
except KeyError as error:
    raise SystemExit(f"unknown pre-commit selector mutation: {mutation}") from error
if text.count(source) != 1:
    raise SystemExit(f"pre-commit mutation anchor is not unique: {source!r}")
path.write_text(text.replace(source, replacement, 1), encoding="utf-8")
PY
}

expect_valid_precommit_config() {
  local name="$1" case_root="$2"
  local output="$case_root/precommit-validate.log"
  if ! (cd "$case_root" && pre-commit validate-config .pre-commit-config.yaml) \
      >"$output" 2>&1
  then
    cat "$output" >&2
    echo "FAIL: $name is not a valid pre-commit config" >&2
    return 1
  fi
}

expect_precommit_skip() {
  local name="$1" case_root="$2"
  local output="$case_root/precommit-run.log"
  case_git "$case_root" add -f -- .pre-commit-config.yaml
  if ! (cd "$case_root" && env -u SKIP \
      pre-commit run docs-generated-drift-self-test --all-files) >"$output" 2>&1
  then
    cat "$output" >&2
    echo "FAIL: $name did not skip successfully under pre-commit" >&2
    return 1
  fi
  if ! grep -Fq "Skipped" "$output"; then
    cat "$output" >&2
    echo "FAIL: $name exited zero without reporting a skipped hook" >&2
    return 1
  fi
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

for skill_path in \
  '.agents/skills/assay-golden-path/SKILL.md' \
  '.claude/skills/assay-golden-path/SKILL.md'
do
  case_root="$SCRATCH/untracked-$(printf '%s' "$skill_path" | tr '/.' '--')"
  seed_case "$case_root"
  case_git "$case_root" rm --cached -- "$skill_path"
  expect_named_failure \
    "untracked skill $skill_path" \
    "$case_root" \
    "skill is not tracked: $skill_path"
done

for skill_path in \
  '.agents/skills/assay-golden-path/SKILL.md' \
  '.claude/skills/assay-golden-path/SKILL.md'
do
  case_root="$SCRATCH/ignored-$(printf '%s' "$skill_path" | tr '/.' '--')"
  seed_case "$case_root"
  printf '%s\n' "$skill_path" >> "$case_root/.gitignore"
  expect_named_failure \
    "ignored tracked skill $skill_path" \
    "$case_root" \
    "tracked skill is ignored: $skill_path"
done

case_root="$SCRATCH/claude-sibling-visible"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".gitignore"
source = ".claude/skills/assay-golden-path/*\n"
text = path.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"Claude skill wildcard is not unique: {source!r}")
path.write_text(text.replace(source, "", 1), encoding="utf-8")
PY
expect_named_failure \
  "Claude skill sibling visible without inner wildcard" \
  "$case_root" \
  "Claude skill sibling is not ignored: .claude/skills/assay-golden-path/OTHER.md"

for skill_path in \
  '.agents/skills/assay-golden-path/SKILL.md' \
  '.claude/skills/assay-golden-path/SKILL.md'
do
  case_root="$SCRATCH/eol-$(printf '%s' "$skill_path" | tr '/.' '--')"
  seed_case "$case_root"
  CASE_ROOT="$case_root" SKILL_PATH="$skill_path" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".gitattributes"
source = f"{os.environ['SKILL_PATH']} text eol=lf\n"
text = path.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"skill eol attribute is not unique: {source!r}")
path.write_text(text.replace(source, "", 1), encoding="utf-8")
PY
  expect_named_failure \
    "skill without LF attribute $skill_path" \
    "$case_root" \
    "skill does not declare eol=lf: $skill_path"
done

case_root="$SCRATCH/symlink-codex-regular-file"
seed_case "$case_root"
skill_path='.agents/skills/assay-golden-path/SKILL.md'
target_path="$case_root/.agents/skills/assay-golden-path/copied-skill.md"
cp "$case_root/$skill_path" "$target_path"
rm "$case_root/$skill_path"
if create_required_symlink \
  "Codex skill symlink to regular copied file" \
  "copied-skill.md" \
  "$case_root/$skill_path"
then
  case_git "$case_root" add -f -- "$skill_path"
  expect_named_failure \
    "Codex skill symlink to regular copied file" \
    "$case_root" \
    "skill evidence must be a regular tracked file, not a symlink"
fi

case_root="$SCRATCH/symlink-claude-dangling"
seed_case "$case_root"
skill_path='.claude/skills/assay-golden-path/SKILL.md'
rm "$case_root/$skill_path"
if create_required_symlink \
  "Claude skill dangling symlink" \
  "missing-skill.md" \
  "$case_root/$skill_path"
then
  case_git "$case_root" add -f -- "$skill_path"
  expect_named_failure \
    "Claude skill dangling symlink" \
    "$case_root" \
    "skill evidence must be a regular tracked file, not a symlink"
fi

case_root="$SCRATCH/symlink-directory"
seed_case "$case_root"
skill_path='.agents/skills/assay-golden-path/SKILL.md'
target_path="$case_root/.agents/skills/assay-golden-path/skill-directory"
mkdir "$target_path"
rm "$case_root/$skill_path"
if create_required_symlink \
  "Codex skill symlink to directory" \
  "skill-directory" \
  "$case_root/$skill_path"
then
  case_git "$case_root" add -f -- "$skill_path"
  expect_named_failure \
    "Codex skill symlink to directory" \
    "$case_root" \
    "skill evidence must be a regular tracked file, not a symlink"
fi

case_root="$SCRATCH/hostile-global-excludes"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

case_root = Path(os.environ["CASE_ROOT"])
ignore = case_root / ".gitignore"
source = ".claude/skills/assay-golden-path/*\n"
text = ignore.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"Claude skill wildcard is not unique: {source!r}")
ignore.write_text(text.replace(source, "", 1), encoding="utf-8")
(case_root / "hostile-excludes").write_text(
    ".claude/skills/assay-golden-path/OTHER.md\n", encoding="utf-8"
)
(case_root / "hostile-gitconfig").write_text(
    "[core]\n\texcludesFile = hostile-excludes\n", encoding="utf-8"
)
PY
GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$case_root/hostile-gitconfig" \
  expect_named_failure \
    "hostile global excludes cannot hide visible Claude sibling" \
    "$case_root" \
    "Claude skill sibling is not ignored: .claude/skills/assay-golden-path/OTHER.md"

case_root="$SCRATCH/hostile-global-attributes"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

case_root = Path(os.environ["CASE_ROOT"])
attributes = case_root / ".gitattributes"
skill = ".agents/skills/assay-golden-path/SKILL.md"
source = f"{skill} text eol=lf\n"
text = attributes.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"skill eol attribute is not unique: {source!r}")
attributes.write_text(text.replace(source, "", 1), encoding="utf-8")
(case_root / "hostile-attributes").write_text(f"{skill} text eol=lf\n", encoding="utf-8")
(case_root / "hostile-gitconfig").write_text(
    "[core]\n\tattributesFile = hostile-attributes\n", encoding="utf-8"
)
PY
GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL="$case_root/hostile-gitconfig" \
  expect_named_failure \
    "hostile global attributes cannot restore LF declaration" \
    "$case_root" \
    "skill does not declare eol=lf: .agents/skills/assay-golden-path/SKILL.md"

for mutation in remove comment; do
  case_root="$SCRATCH/workflow-$mutation-gitattributes"
  seed_case "$case_root"
  WORKFLOW_PATH='.gitattributes' CASE_ROOT="$case_root" MUTATION="$mutation" python3 - <<'PY'
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
    "workflow $mutation mutation for .gitattributes" \
    "$case_root" \
    "kernel-matrix workflow does not cover skill contract path: .gitattributes"
done

for hook_id in docs-generated-drift agent-golden-path-skill-contract; do
  case_root="$SCRATCH/hook-gitattributes-$hook_id"
  seed_case "$case_root"
  CASE_ROOT="$case_root" HOOK_ID="$hook_id" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".pre-commit-config.yaml"
hook_id = os.environ["HOOK_ID"]
text = path.read_text(encoding="utf-8")
start = text.find(f"      - id: {hook_id}\n")
if start < 0:
    raise SystemExit(f"hook is not declared: {hook_id}")
end = text.find("      - id: ", start + 1)
if end < 0:
    end = len(text)
block = text[start:end]
source = "|\\.gitattributes"
if block.count(source) != 1:
    raise SystemExit(f"hook gitattributes path is not unique: {hook_id}")
path.write_text(text[:start] + block.replace(source, "", 1) + text[end:], encoding="utf-8")
PY
  expect_named_failure \
    "hook omits .gitattributes $hook_id" \
    "$case_root" \
    "${hook_id} does not cover .gitattributes"
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
    "test-check-docs-generated-drift(?:-safety)?|lib/drift-tree-snapshot)\\.sh|scripts/docs/"
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
mutate_precommit_selector "$case_root" hook-files
expect_valid_precommit_config "golden-path generator outside generated-docs drift self-test" "$case_root"
expect_precommit_skip "golden-path generator outside generated-docs drift self-test" "$case_root"
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

declare -a executor_mutations=(
  "comment-lint|commented lint job|kernel-matrix workflow must declare exactly one active lint job"
  "delete-canonical-pre-commit|missing canonical pre-commit executor|kernel-matrix lint job has no canonical pre-commit executor"
  "pre-commit-files|pre-commit files executor|kernel-matrix lint pre-commit command is noncanonical"
  "pre-commit-hook-stage|pre-commit hook-stage executor|kernel-matrix lint pre-commit command is noncanonical"
  "lint-step-condition|conditional lint executor|kernel-matrix lint executor must not be conditional"
  "lint-step-continue-on-error|non-failing lint executor|kernel-matrix lint executor must fail closed"
  "lint-needs|lint job dependency|kernel-matrix lint job must not depend on another job"
  "lint-runner|noncanonical lint runner|kernel-matrix lint job must run on ubuntu-latest"
)

for executor_case in "${executor_mutations[@]}"; do
  IFS='|' read -r mutation name expected <<<"$executor_case"
  case_root="$SCRATCH/executor-$mutation"
  seed_case "$case_root"
  mutate_workflow "$case_root" "$mutation"
  expect_named_failure "$name" "$case_root" "$expected"
done

declare -a structural_workflow_mutations=(
  "duplicate-path-entry|duplicate pull-request path entry|kernel-matrix pull_request.paths duplicates entry: crates/assay-ebpf/**"
  "spaced-types|spaced pull-request types key|kernel-matrix pull_request must not declare types"
  "lint-step-spaced-condition|spaced lint-step condition key|kernel-matrix lint executor must not be conditional"
  "lint-step-spaced-continue-on-error|spaced lint-step fail-open key|kernel-matrix lint executor must fail closed"
)

for structural_case in "${structural_workflow_mutations[@]}"; do
  IFS='|' read -r mutation name expected <<<"$structural_case"
  case_root="$SCRATCH/structural-workflow-$mutation"
  seed_case "$case_root"
  mutate_workflow "$case_root" "$mutation"
  expect_structural_failure "$name" "$case_root" "$expected"
done

case_root="$SCRATCH/structural-precommit-spaced-stages"
seed_case "$case_root"
mutate_precommit_selector "$case_root" spaced-stages
expect_valid_precommit_config "spaced self-test stages key" "$case_root"
expect_structural_failure \
  "spaced self-test stages key" \
  "$case_root" \
  "generated-docs drift self-test must run at the default pre-commit stage"

declare -a selector_mutations=(
  "root-files|root files selector|generated-docs drift self-test root files selector excludes its golden-path generator"
  "root-spaced-files|spaced root files selector|generated-docs drift self-test root files selector excludes its golden-path generator"
  "root-exclude|root exclude selector|generated-docs drift self-test root exclude selector excludes its golden-path generator"
  "root-spaced-exclude|spaced root exclude selector|generated-docs drift self-test root exclude selector excludes its golden-path generator"
  "hook-spaced-files|spaced hook files selector skip|generated-docs drift self-test does not cover its golden-path generator"
  "hook-exclude|hook exclude selector|generated-docs drift self-test exclude selector excludes its golden-path generator"
  "hook-exclude-inline-comment|hook exclude selector with inline comment|generated-docs drift self-test exclude selector excludes its golden-path generator"
  "hook-spaced-exclude|spaced hook exclude selector|generated-docs drift self-test exclude selector excludes its golden-path generator"
  "hook-types|hook types selector|generated-docs drift self-test types selector excludes its golden-path generator"
  "hook-spaced-types|spaced hook types selector|generated-docs drift self-test types selector excludes its golden-path generator"
  "hook-types-or|hook types_or selector|generated-docs drift self-test types_or selector excludes its golden-path generator"
  "hook-spaced-types-or|spaced hook types_or selector|generated-docs drift self-test types_or selector excludes its golden-path generator"
  "hook-exclude-types|hook exclude_types selector|generated-docs drift self-test exclude_types selector excludes its golden-path generator"
  "hook-spaced-exclude-types|spaced hook exclude_types selector|generated-docs drift self-test exclude_types selector excludes its golden-path generator"
)

for selector_case in "${selector_mutations[@]}"; do
  IFS='|' read -r mutation name expected <<<"$selector_case"
  case_root="$SCRATCH/structural-selector-$mutation"
  seed_case "$case_root"
  mutate_precommit_selector "$case_root" "$mutation"
  expect_valid_precommit_config "$name" "$case_root"
  expect_precommit_skip "$name" "$case_root"
  expect_structural_failure "$name" "$case_root" "$expected"
done

case_root="$SCRATCH/structural-trigger-executable-mode"
seed_case "$case_root"
chmod +x "$case_root/scripts/docs/generate-agent-golden-path.py"
case_git "$case_root" add -- scripts/docs/generate-agent-golden-path.py
expect_structural_failure \
  "executable golden-path generator trigger" \
  "$case_root" \
  "generated-docs drift self-test trigger must remain a tracked 100644 file"

case_root="$SCRATCH/separate-pre-push-pre-commit-step"
seed_case "$case_root"
CASE_ROOT="$case_root" python3 - <<'PY'
import os
from pathlib import Path

path = Path(os.environ["CASE_ROOT"]) / ".github/workflows/kernel-matrix.yml"
source = (
    "      - name: Run pre-commit (all files)\n"
    "        shell: bash\n"
    "        run: |\n"
    "          set -euo pipefail\n"
    "          pre-commit run --all-files --show-diff-on-failure\n"
)
addition = (
    source
    + "\n"
    + "      - name: Run pre-push pre-commit\n"
    + "        shell: bash\n"
    + "        run: pre-commit run --hook-stage pre-push --all-files\n"
)
text = path.read_text(encoding="utf-8")
if text.count(source) != 1:
    raise SystemExit(f"canonical pre-commit step is not unique: {source!r}")
path.write_text(text.replace(source, addition, 1), encoding="utf-8")
PY
expect_named_success "separate pre-push pre-commit step" "$case_root"

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
lint_steps_anchor = (
    "  lint:\n    name: Lint (pre-commit)\n    runs-on: ubuntu-latest\n"
    "    timeout-minutes: 10\n    permissions:\n      contents: read\n    steps:\n"
)
if text.count(lint_steps_anchor) != 1:
    raise SystemExit(f"lint steps anchor is not unique: {lint_steps_anchor!r}")
text = text.replace(source, replacement, 1)
text = text.replace(
    lint_steps_anchor,
    lint_steps_anchor.replace(
        "    steps:\n",
        "    outputs:\n      - run: echo non-step-parser-sentinel\n    steps:\n",
    ),
    1,
)
path.write_text(text, encoding="utf-8")
PY
inner_probe_count_path="$case_root/inner-structural-probe-count"
if ! CASE_ROOT="$case_root" STRUCTURAL_COUNT_PATH="$inner_probe_count_path" python3 - <<'PY'
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
steps_section = "    outputs:\n      - run: echo non-step-parser-sentinel\n    steps:\n"
if workflow.count(steps_section) != 1:
    raise SystemExit(f"injected steps section is not unique: {steps_section!r}")

structural_probe_count = 0

def expect_parser_failure(name, candidate, expected):
    global structural_probe_count
    try:
        module.parse_kernel_matrix_workflow(candidate)
    except AssertionError as error:
        if expected not in str(error):
            raise SystemExit(f"{name} reached {error!s}, not {expected!r}") from error
    else:
        raise SystemExit(f"{name} was accepted")
    structural_probe_count += 1
    print(f"ok    structural probe: {name}")

expect_parser_failure(
    "missing lint steps",
    workflow.replace(
        steps_section,
        "    outputs:\n      - run: echo non-step-parser-sentinel\n",
        1,
    ),
    "kernel-matrix lint job is missing required key: steps",
)
expect_parser_failure(
    "duplicate lint steps",
    workflow.replace(steps_section, steps_section + "    steps:\n", 1),
    "kernel-matrix lint job duplicates key: steps",
)
contract = module.parse_kernel_matrix_workflow(workflow)
if sum(step.shell_lines == ("echo inline-parser-sentinel",) for step in contract.lint_steps) != 1:
    raise SystemExit("parser did not retain exactly one inline run sentinel")
if any("echo non-step-parser-sentinel" in step.shell_lines for step in contract.lint_steps):
    raise SystemExit("parser treated a list outside steps as an executable step")

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

Path(os.environ["STRUCTURAL_COUNT_PATH"]).write_text(
    f"{structural_probe_count}\n", encoding="ascii"
)
PY
then
  echo "FAIL: inline run parser probe failed" >&2
  exit 1
fi
if [[ ! -f "$inner_probe_count_path" ]]; then
  echo "FAIL: inline parser probe did not report its structural probe count" >&2
  exit 1
fi
inner_probe_count="$(cat "$inner_probe_count_path")"
if [[ ! "$inner_probe_count" =~ ^[0-9]+$ ]]; then
  echo "FAIL: inline parser probe reported invalid count: $inner_probe_count" >&2
  exit 1
fi
structural_probe_count=$((structural_probe_count + inner_probe_count))
record_case_pass "inline run parser probe"

if (( case_count != EXPECTED_CASES )); then
  echo "FAIL: agent golden-path hardening expected $EXPECTED_CASES case(s), executed $case_count" >&2
  exit 1
fi

if (( structural_probe_count != EXPECTED_STRUCTURAL_PROBES )); then
  echo "FAIL: agent golden-path hardening expected $EXPECTED_STRUCTURAL_PROBES structural probe(s), executed $structural_probe_count" >&2
  exit 1
fi

echo "agent golden-path hardening: $case_count case(s) observed; $structural_probe_count structural probe(s) observed"
