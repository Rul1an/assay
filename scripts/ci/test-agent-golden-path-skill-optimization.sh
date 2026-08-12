#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/ci/lib/golden-path-fixture-staging.sh
source "$SCRIPT_DIR/lib/golden-path-fixture-staging.sh"

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

stage_golden_path_fixtures "$SCRATCH" "$ROOT"
scratch_git -c init.defaultBranch=main init -q
scratch_git -c core.excludesFile= -c core.attributesFile= \
  add -f -- .

check_no_asserts() {
  python3 - "$@" <<'PY'
import ast
from pathlib import Path
import sys

if len(sys.argv) != 3:
    raise SystemExit("optimizer gate must scan exactly two Python files")
for raw in sys.argv[1:]:
    path = Path(raw)
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    asserts = [node for node in ast.walk(tree) if isinstance(node, ast.Assert)]
    if asserts:
        lines = ", ".join(str(node.lineno) for node in asserts)
        raise SystemExit(f"optimizer-erased assert in {path}: lines {lines}")
PY
}

check_no_asserts \
  "$SCRATCH/scripts/ci/test-agent-golden-path-skill.py" \
  "$SCRATCH/scripts/docs/generate-agent-golden-path.py"

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

assert_validator="$SCRATCH/scripts/ci/validator-with-assert.py"
cp "$SCRATCH/scripts/ci/test-agent-golden-path-skill.py" "$assert_validator"
python3 - "$assert_validator" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
source = path.read_text(encoding="utf-8")
guard = '''if contract.get("schema") != "assay.agent_golden_path.v1":
        fail("unexpected golden-path contract schema")'''
assertion = '''assert contract.get("schema") == "assay.agent_golden_path.v1"'''
if guard not in source:
    raise SystemExit("optimizer mutation could not find contract schema guard")
path.write_text(source.replace(guard, assertion, 1), encoding="utf-8")
PY

output="$SCRATCH/inserted-assert.log"
if check_no_asserts \
  "$assert_validator" \
  "$SCRATCH/scripts/docs/generate-agent-golden-path.py" >"$output" 2>&1; then
  echo "FAIL: optimizer gate accepted an inserted validator assert" >&2
  exit 1
fi

if ! grep -Fq "optimizer-erased assert" "$output"; then
  cat "$output" >&2
  echo "FAIL: inserted validator assert did not reach the AST gate" >&2
  exit 1
fi

echo "optimized Python preserves golden-path contract validation"
