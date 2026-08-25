#!/usr/bin/env bash
# Mutation battery for the closed three-workflow CodeQL upload-sarif pin set.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-codeql-upload-sarif-lockstep.py"
PRECOMMIT=".pre-commit-config.yaml"
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
  cp "$ROOT/$PRECOMMIT" "$dest/$PRECOMMIT"
  for path in "${WORKFLOWS[@]}"; do
    cp "$ROOT/$path" "$dest/$path"
  done
}

check_hook_scope() {
  local root="$1"
  python3 - "$root/$PRECOMMIT" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
block = text.split("- id: codeql-upload-sarif-lockstep", 1)[1].split("\n      - id:", 1)[0]
match = re.search(r"^[ \t]*files:[ \t]*(.+)$", block, re.MULTILINE)
if match is None:
    raise SystemExit("CodeQL lockstep hook has no files selector")
pattern = match.group(1).strip()
required = (
    ".github/workflows/fourth-upload.yml",
    ".github/workflows/fourth-upload.yaml",
    "scripts/ci/check-codeql-upload-sarif-lockstep.py",
    "scripts/ci/test-codeql-upload-sarif-lockstep.sh",
    ".pre-commit-config.yaml",
)
missing = [path for path in required if re.search(pattern, path) is None]
if missing:
    raise SystemExit(f"CodeQL lockstep hook does not trigger for: {', '.join(missing)}")
PY
}

run_hook_scope_case() {
  local name="$1" root="$2" expected="$3" status=0
  check_hook_scope "$root" >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
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
run_hook_scope_case hook-scope-covers-all-workflows "$case_root" 0

case_root="$scratch/narrow-hook-scope"
seed "$case_root"
mutate_once \
  "$case_root/$PRECOMMIT" \
  'files: ^(\.github/workflows/.*\.ya?ml|scripts/ci/(check-codeql-upload-sarif-lockstep\.py|test-codeql-upload-sarif-lockstep\.sh)|\.pre-commit-config\.yaml)$' \
  'files: ^(\.github/workflows/(assay-security|openssf-scorecard|osv-scanner-scheduled)\.yml|scripts/ci/(check-codeql-upload-sarif-lockstep\.py|test-codeql-upload-sarif-lockstep\.sh)|\.pre-commit-config\.yaml)$'
run_hook_scope_case narrow-hook-scope-is-refused "$case_root" 1

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

case_root="$scratch/quoted-duplicate"
seed "$case_root"
cat >>"$case_root/.github/workflows/assay-security.yml" <<'YAML'

# Mutation: quoted uses values are valid workflow YAML and remain active.
uses: "github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28" # v4.37.8
YAML
run_case quoted-duplicate-is-refused "$case_root" 1

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

case_root="$scratch/quoted-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/quoted-upload.yaml" <<'YAML'
name: Mutated quoted CodeQL upload
jobs:
  upload:
    steps:
      - uses: 'github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28' # v4.37.8
YAML
run_case quoted-foreign-workflow-callsite-is-refused "$case_root" 1

case_root="$scratch/flow-mapping-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/flow-upload.yml" <<'YAML'
name: Mutated flow-mapping CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - {uses: github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28}
YAML
run_case flow-mapping-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/quoted-key-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/quoted-key-upload.yml" <<'YAML'
name: Mutated quoted-key CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - "uses": github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28
YAML
run_case quoted-key-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/spaced-colon-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/spaced-colon-upload.yml" <<'YAML'
name: Mutated spaced-colon CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses : github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28
YAML
run_case spaced-colon-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/folded-scalar-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/folded-upload.yml" <<'YAML'
name: Mutated folded-scalar CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses: >-
          github/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28
YAML
run_case folded-scalar-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/unicode-escaped-action"
seed "$case_root"
cat >"$case_root/.github/workflows/unicode-escaped-upload.yml" <<'YAML'
name: Mutated Unicode-escaped CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses: "github/codeql-action/upload-sarif\u0040db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"
YAML
run_case unicode-escaped-action-is-refused "$case_root" 1

case_root="$scratch/hex-escaped-action"
seed "$case_root"
cat >"$case_root/.github/workflows/hex-escaped-upload.yml" <<'YAML'
name: Mutated hex-escaped CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses: "github/codeql-action/upload-sarif\x40db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"
YAML
run_case hex-escaped-action-is-refused "$case_root" 1

case_root="$scratch/case-variant-action"
seed "$case_root"
cat >"$case_root/.github/workflows/case-variant-upload.yml" <<'YAML'
name: Mutated case-variant CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses: GitHub/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28
YAML
run_case case-variant-action-is-refused "$case_root" 1

case_root="$scratch/unicode-escaped-identity"
seed "$case_root"
cat >"$case_root/.github/workflows/unicode-identity-upload.yml" <<'YAML'
name: Mutated Unicode-escaped CodeQL identity
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses: "\u0067ithub/codeql-action/upload-sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"
YAML
run_case unicode-escaped-identity-is-refused "$case_root" 1

case_root="$scratch/escaped-line-break-action"
seed "$case_root"
cat >"$case_root/.github/workflows/escaped-break-upload.yml" <<'YAML'
name: Mutated escaped-line-break CodeQL upload
on: workflow_dispatch
jobs:
  upload:
    runs-on: ubuntu-latest
    steps:
      - uses: "github/codeql-action/upload-\
          sarif@db488ddef3bf6cb639b32c2e9a7c0a7ea8271d28"
YAML
run_case escaped-line-break-action-is-refused "$case_root" 1

printf 'PASS: CodeQL upload-sarif lockstep battery\n'
