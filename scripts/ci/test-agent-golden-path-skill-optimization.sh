#!/usr/bin/env bash
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

scratch_git() {
  env \
    -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
    -u GIT_COMMON_DIR \
    -u GIT_DIR \
    -u GIT_INDEX_FILE \
    -u GIT_OBJECT_DIRECTORY \
    -u GIT_WORK_TREE \
    GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
    git -C "$SCRATCH" "$@"
}

mkdir -p \
  "$SCRATCH/scripts/ci" \
  "$SCRATCH/docs/generated" \
  "$SCRATCH/.agents/skills/assay-golden-path" \
  "$SCRATCH/.claude/skills/assay-golden-path"

cp "$ROOT/scripts/ci/test-agent-golden-path-skill.py" "$SCRATCH/scripts/ci/"
cp "$ROOT/.gitignore" "$SCRATCH/"
cp "$ROOT/.gitattributes" "$SCRATCH/"
cp "$ROOT/docs/generated/agent-golden-path.json" "$SCRATCH/docs/generated/"
cp "$ROOT/.agents/skills/assay-golden-path/SKILL.md" \
  "$SCRATCH/.agents/skills/assay-golden-path/"
cp "$ROOT/.claude/skills/assay-golden-path/SKILL.md" \
  "$SCRATCH/.claude/skills/assay-golden-path/"
scratch_git -c init.defaultBranch=main init -q
scratch_git -c core.excludesFile= -c core.attributesFile= \
  add -f -- .

python3 - "$SCRATCH/docs/generated/agent-golden-path.json" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
contract = json.loads(path.read_text(encoding="utf-8"))
contract["schema"] = "assay.agent_golden_path.invalid"
path.write_text(json.dumps(contract, indent=2) + "\n", encoding="utf-8")
PY

output="$SCRATCH/optimized-validator.log"
if python3 -OO "$SCRATCH/scripts/ci/test-agent-golden-path-skill.py" >"$output" 2>&1; then
  echo "FAIL: optimized Python accepted an invalid golden-path schema" >&2
  exit 1
fi

if ! grep -Fq "unexpected golden-path contract schema" "$output"; then
  cat "$output" >&2
  echo "FAIL: invalid schema did not reach the named contract guard" >&2
  exit 1
fi

echo "optimized Python preserves golden-path contract validation"
