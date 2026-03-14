#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-wave42-context-envelope-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave42-context-envelope-step3.md"
  "scripts/ci/review-wave42-context-envelope-step3.sh"
)

echo "[review] step3 docs+script-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave42 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave42 Step3: $f"
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
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] rerun Step2 invariants"

echo "[review] Wave42 context-envelope markers"
for marker in \
  'decision_context_contract_version' \
  'context_payload_state' \
  'required_context_fields' \
  'missing_context_fields' \
  'ContextPayloadState' \
  'DECISION_CONTEXT_CONTRACT_VERSION_V1' \
  'project_context_contract'
do
  rg -n "$marker" \
    crates/assay-core/src/mcp/decision.rs \
    crates/assay-core/src/mcp/decision/context_contract.rs \
    crates/assay-core/tests/decision_emit_invariant.rs >/dev/null || {
    echo "FAIL: missing Wave42 context-envelope marker: $marker"
    exit 1
  }
done

echo "[review] deterministic context completeness markers"
for marker in \
  'CompleteEnvelope' \
  'PartialEnvelope' \
  'AbsentEnvelope' \
  'lane' \
  'principal' \
  'auth_context_summary' \
  'approval_state'
do
  rg -n "$marker" \
    crates/assay-core/src/mcp/decision/context_contract.rs \
    crates/assay-core/tests/decision_emit_invariant.rs >/dev/null || {
    echo "FAIL: missing deterministic context completeness marker: $marker"
    exit 1
  }
done

echo "[review] existing replay/decision markers remain present"
for marker in \
  'DecisionOutcomeKind' \
  'OutcomeCompatState' \
  'fulfillment_decision_path' \
  'decision_consumer_contract_version' \
  'consumer_payload_state'
do
  rg -n "$marker" \
    crates/assay-core/src/mcp/decision.rs \
    crates/assay-core/tests/decision_emit_invariant.rs >/dev/null || {
    echo "FAIL: missing existing replay/decision marker: $marker"
    exit 1
  }
done

echo "[review] no runtime behavior scope creep"
if rg -n 'runtime behavior change|new runtime capability|policy-engine|control-plane|auth transport|enforcement semantics' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: non-goal scope markers detected in implementation scope"
  rg -n 'runtime behavior change|new runtime capability|policy-engine|control-plane|auth transport|enforcement semantics' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned replay/decision tests"
cargo test -p assay-core context_contract
cargo test -p assay-core --test decision_emit_invariant test_event_contains_required_fields -- --exact
cargo test -p assay-core --test fulfillment_normalization
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server --test auth_integration

echo "[review] PASS"
