#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave19-coverage-command.md"
  "docs/contributing/SPLIT-CHECKLIST-coverage-command-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step1.md"
  "scripts/ci/review-coverage-command-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, coverage-command ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave19 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave19 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^crates/assay-cli/src/cli/commands/coverage(\.rs|/)' >/dev/null; then
  echo "FAIL: Wave19 Step1 must not change coverage command code"
  exit 1
fi

if git ls-files --others --exclude-standard -- \
  'crates/assay-cli/src/cli/commands/coverage.rs' \
  'crates/assay-cli/src/cli/commands/coverage/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under coverage command subtree are not allowed in Wave19 Step1"
  git ls-files --others --exclude-standard -- \
    'crates/assay-cli/src/cli/commands/coverage.rs' \
    'crates/assay-cli/src/cli/commands/coverage/**' | sed 's/^/  - /'
  exit 1
fi

cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings

cargo test -p assay-cli coverage_contract_generates_valid_report_from_basic_jsonl -- --exact
cargo test -p assay-cli coverage_out_md_writes_json_and_markdown_artifacts -- --exact
cargo test -p assay-cli coverage_declared_tools_file_union_with_flags -- --exact

echo "[review] PASS"
