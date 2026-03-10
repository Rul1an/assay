#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-typed-decisions-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step3.md"
  "scripts/ci/review-wave24-typed-decisions-step3.sh"
)

echo "[review] step3 docs+script-only diff vs $BASE_REF + workflow-ban"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave24 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave24 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] rerun Step2 invariants"

echo "[review] typed decision markers"
rg -n 'allow_with_obligations|deny_with_alert' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing typed decision markers allow_with_obligations / deny_with_alert"
  exit 1
}

rg -n 'AllowWithWarning' crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing AllowWithWarning compatibility path"
  exit 1
}

echo "[review] Decision Event v2 field markers"
rg -n 'policy_version|policy_digest|obligations|approval_state|lane|principal|auth_context_summary' \
  crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing Decision Event v2 field markers"
  exit 1
}

echo "[review] required legacy decision markers still present"
rg -n 'tool_classes|matched_tool_classes|match_basis|matched_rule|reason_code' \
  crates/assay-core/src/mcp >/dev/null || {
  echo "FAIL: missing existing decision-event fields"
  exit 1
}

echo "[review] no obligations execution in this wave"
if rg -n 'approval_required|redact_args|restrict_scope|obligation_fulfillment|execute_obligation' \
  crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
  | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/' >/dev/null; then
  echo "FAIL: obligations execution markers detected in implementation scope"
  rg -n 'approval_required|redact_args|restrict_scope|obligation_fulfillment|execute_obligation' \
    crates/assay-core/src/mcp crates/assay-cli/src/cli/commands crates/assay-mcp-server \
    | rg -v 'SPLIT-|PLAN-ADR|ADR-032|docs/'
  exit 1
fi

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings

echo "[review] pinned tests"
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration

echo "[review] PASS"
