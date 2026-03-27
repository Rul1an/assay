#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/mcp/decision.rs"
  "crates/assay-core/src/mcp/decision/replay_diff.rs"
  "crates/assay-core/tests/replay_diff_contract.rs"

  "docs/contributing/SPLIT-CHECKLIST-wave38-replay-diff-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave38-replay-diff-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave38-replay-diff-step2.md"

  "scripts/ci/review-wave38-replay-diff-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave38 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave38 Step2: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] no untracked files under bounded runtime scope"
for p in \
  "crates/assay-core/src/mcp" \
  "crates/assay-core/tests" \
  "crates/assay-cli/src/cli/commands" \
  "crates/assay-mcp-server"
do
  while IFS= read -r uf; do
    [[ -z "${uf:-}" ]] && continue
    allowed="false"
    for a in "${ALLOWLIST[@]}"; do
      [[ "$uf" == "$a" ]] && allowed="true" && break
    done
    if [[ "$allowed" != "true" ]]; then
      echo "FAIL: untracked file present under $p: $uf"
      exit 1
    fi
  done < <(git ls-files --others --exclude-standard -- "$p")
done

echo "[review] replay diff markers"
for marker in \
  'ReplayDiffBasis' \
  'ReplayDiffBucket' \
  'basis_from_decision_data' \
  'classify_replay_diff' \
  'Unchanged' \
  'Stricter' \
  'Looser' \
  'Reclassified' \
  'EvidenceOnly'
do
  rg -n "$marker" crates/assay-core/src/mcp/decision.rs crates/assay-core/src/mcp/decision/replay_diff.rs >/dev/null || {
    echo "FAIL: missing replay-diff marker: $marker"
    exit 1
  }
done

echo "[review] existing convergence markers remain present"
for marker in \
  'DecisionOutcomeKind' \
  'DecisionOrigin' \
  'OutcomeCompatState' \
  'fulfillment_decision_path' \
  'normalization_version'
do
  rg -n "$marker" crates/assay-core/src/mcp/decision.rs crates/assay-core/src/mcp/decision/replay_diff.rs >/dev/null || {
    echo "FAIL: missing existing convergence marker: $marker"
    exit 1
  }
done

echo "[review] no scope creep into non-goals"
if rg -n 'new obligation type|runtime enforcement expansion|policy backend replacement|control-plane|auth transport' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: non-goal scope markers detected in implementation scope"
  rg -n 'new obligation type|runtime enforcement expansion|policy backend replacement|control-plane|auth transport' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core --test replay_diff_contract
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -p assay-core --test fulfillment_normalization
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server --test auth_integration

echo "[review] PASS"
