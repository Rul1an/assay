#!/usr/bin/env bash
# Mutation battery for the closed three-workflow CodeQL upload-sarif pin set.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-codeql-upload-sarif-lockstep.py"
WORKFLOWS=(
  .github/workflows/assay-security.yml
  .github/workflows/openssf-scorecard.yml
  .github/workflows/osv-scanner-scheduled.yml
)

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

seed() {
  local dest="$1" path
  mkdir -p "$dest/scripts/ci" "$dest/.github/workflows"
  cp "$ROOT/$CHECKER" "$dest/$CHECKER"
  for path in "${WORKFLOWS[@]}"; do
    cp "$ROOT/$path" "$dest/$path"
  done
}

run_case() {
  local name="$1" root="$2" expected="$3" status=0
  (cd "$root" && python3 "$CHECKER") >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

mutate_once() {
  local path="$1" old="$2" new="$3"
  python3 - "$path" "$old" "$new" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old, new = sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation subject count is {text.count(old)}, want 1: {old}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

case_root="$scratch/control"
seed "$case_root"
run_case control-is-green "$case_root" 0

case_root="$scratch/one-laggard"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/openssf-scorecard.yml" \
  "db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28" \
  "d1ba80a13dd99fba24a470575428917156a28b43"
run_case one-laggard-is-refused "$case_root" 1

case_root="$scratch/tag-drift"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/assay-security.yml" \
  "# v4.37.8" \
  "# v4.37.5"
run_case tag-drift-is-refused "$case_root" 1

case_root="$scratch/invoke-bypass"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/osv-scanner-scheduled.yml" \
  "uses: github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28 # v4.37.8" \
  "run: echo bypassed-upload"
run_case invoke-bypass-is-refused "$case_root" 1

case_root="$scratch/extra-callsite"
seed "$case_root"
cat >>"$case_root/.github/workflows/assay-security.yml" <<'YAML'

# Mutation: a second active upload is outside the closed one-callsite contract.
uses: github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28 # v4.37.8
YAML
run_case extra-callsite-is-refused "$case_root" 1

case_root="$scratch/foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/fourth-upload.yml" <<'YAML'
name: Mutated fourth CodeQL upload
jobs:
  upload:
    steps:
      - uses: github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28 # v4.37.8
YAML
run_case foreign-workflow-callsite-is-refused "$case_root" 1

printf 'PASS: CodeQL upload-sarif lockstep battery\n'
