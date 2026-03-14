#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/mcp/decision.rs"
  "crates/assay-core/src/mcp/decision/consumer_contract.rs"
  "crates/assay-core/src/mcp/decision/replay_diff.rs"
  "crates/assay-core/tests/replay_diff_contract.rs"
  "crates/assay-core/tests/decision_emit_invariant.rs"

  "docs/contributing/SPLIT-CHECKLIST-wave41-consumer-hardening-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave41-consumer-hardening-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave41-consumer-hardening-step2.md"

  "scripts/ci/review-wave41-consumer-hardening-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave41 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave41 Step2: $f"
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

echo "[review] Wave41 consumer-hardening markers"
for marker in \
  'decision_consumer_contract_version' \
  'consumer_read_path' \
  'consumer_fallback_applied' \
  'consumer_payload_state' \
  'required_consumer_fields' \
  'ConsumerReadPath' \
  'ConsumerPayloadState' \
  'DECISION_CONSUMER_CONTRACT_VERSION_V1' \
  'project_consumer_contract'
do
  rg -n "$marker" \
    crates/assay-core/src/mcp/decision.rs \
    crates/assay-core/src/mcp/decision/consumer_contract.rs \
    crates/assay-core/src/mcp/decision/replay_diff.rs >/dev/null || {
    echo "FAIL: missing Wave41 consumer-hardening marker: $marker"
    exit 1
  }
done

echo "[review] deterministic consumer precedence markers"
for marker in \
  'ConvergedDecision' \
  'CompatibilityMarkers' \
  'LegacyDecision' \
  'decision_outcome_kind' \
  'classification_source' \
  'legacy_shape_detected'
do
  rg -n "$marker" \
    crates/assay-core/src/mcp/decision/consumer_contract.rs \
    crates/assay-core/tests/replay_diff_contract.rs >/dev/null || {
    echo "FAIL: missing deterministic consumer precedence marker: $marker"
    exit 1
  }
done

echo "[review] existing replay/decision markers remain present"
for marker in \
  'ReplayDiffBasis' \
  'ReplayDiffBucket' \
  'DecisionOutcomeKind' \
  'OutcomeCompatState' \
  'fulfillment_decision_path' \
  'decision_basis_version' \
  'classification_source'
do
  rg -n "$marker" \
    crates/assay-core/src/mcp/decision.rs \
    crates/assay-core/src/mcp/decision/replay_diff.rs >/dev/null || {
    echo "FAIL: missing existing replay/decision marker: $marker"
    exit 1
  }
done

echo "[review] no runtime behavior scope creep"
if git diff --name-only "$BASE_REF"...HEAD -- \
  crates/assay-core/src/mcp/tool_call_handler \
  crates/assay-core/src/mcp/policy \
  crates/assay-cli/src/cli/commands/mcp.rs \
  crates/assay-mcp-server \
  | rg -n '.' >/dev/null; then
  echo "FAIL: Wave41 Step2 must not modify runtime behavior paths outside decision/replay consumer scope"
  git diff --name-only "$BASE_REF"...HEAD -- \
    crates/assay-core/src/mcp/tool_call_handler \
    crates/assay-core/src/mcp/policy \
    crates/assay-cli/src/cli/commands/mcp.rs \
    crates/assay-mcp-server
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned replay/decision tests"
cargo test -p assay-core consumer_contract
cargo test -p assay-core --test replay_diff_contract
cargo test -p assay-core --test decision_emit_invariant test_event_contains_required_fields -- --exact
cargo test -p assay-core --test fulfillment_normalization
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server --test auth_integration

echo "[review] PASS"
