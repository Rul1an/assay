#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md"
  "docs/contributing/SPLIT-CHECKLIST-typed-decisions-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step1.md"
  "scripts/ci/review-wave24-typed-decisions-step1.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp"
  "crates/assay-cli/src/cli/commands/mcp.rs"
  "crates/assay-cli/src/cli/commands/coverage"
  "crates/assay-cli/src/cli/commands/session_state_window.rs"
  "crates/assay-mcp-server"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave24 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave24 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave24 Step1 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

echo "[review] frozen paths must not contain untracked files"
for p in "${FROZEN_PATHS[@]}"; do
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
rg -n '^# SPLIT PLAN — Wave24 Typed Decisions and Decision Event v2$' \
  docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

rg -n '`allow_with_obligations`' \
  docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md >/dev/null || {
  echo "FAIL: missing typed decision marker allow_with_obligations"
  exit 1
}

rg -n 'AllowWithWarning' \
  docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md >/dev/null || {
  echo "FAIL: missing AllowWithWarning compatibility marker"
  exit 1
}

rg -n 'policy_version|policy_digest|obligations|approval_state|lane|principal|auth_context_summary' \
  docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md >/dev/null || {
  echo "FAIL: missing Decision Event v2 field markers"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
