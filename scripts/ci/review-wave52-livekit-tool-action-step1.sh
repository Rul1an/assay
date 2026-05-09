#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"

allowed_pattern='^(crates/assay-cli/src/cli/commands/evidence/livekit_tool_action\.rs|crates/assay-cli/receipt-schemas/(inputs/livekit-function-tools-executed-export\.v1\.schema\.json|receipts/livekit\.tool-action\.v1\.schema\.json)|docs/reference/receipt-schemas/(inputs/livekit-function-tools-executed-export\.v1\.schema\.json|receipts/livekit\.tool-action\.v1\.schema\.json)|docs/reference/cli/evidence\.md|docs/architecture/PLAN-P47-LIVEKIT-ACTED-FAMILY-TOOL-ACTION-RECEIPTS-2026q2\.md|examples/livekit-tool-action-evidence/.*|docs/contributing/SPLIT-PLAN-wave52-livekit-tool-action\.md|docs/contributing/SPLIT-CHECKLIST-wave52-livekit-tool-action-step1\.md|docs/contributing/SPLIT-MOVE-MAP-wave52-livekit-tool-action-step1\.md|docs/contributing/SPLIT-REVIEW-PACK-wave52-livekit-tool-action-step1\.md|scripts/ci/review-wave52-livekit-tool-action-step1\.sh|docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2\.md|docs/security/OWASP-MCP-TOP10-TEST-MAP\.md|scripts/ci/optional-public-api-drift\.sh|scripts/ci/mutation-smoke-pure-modules\.sh|scripts/ci/review-week8-sota-gates\.sh)$'

if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  echo "FAIL: BASE_REF ${BASE_REF} is not available; fetch it or set BASE_REF to a local commit"
  exit 1
fi

collect_changed() {
  {
    git diff --name-only "$BASE_REF"...HEAD
    git diff --name-only || true
    git diff --cached --name-only || true
    git ls-files --others --exclude-standard || true
  } | sed '/^$/d' | sort -u
}

echo "[review] Step1 allowlist vs $BASE_REF + local changes"
changed="$(collect_changed)"
if [ -n "$changed" ]; then
  non_allowed="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
  if [ -n "$non_allowed" ]; then
    echo "FAIL: non-allowlisted files changed:"
    printf '%s\n' "$non_allowed"
    exit 1
  fi
fi

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: Step1 must not touch workflows"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-ebpf/src/vmlinux\.rs$' >/dev/null; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/' >/dev/null; then
  echo "FAIL: Step1 must not split livekit_tool_action into modules yet"
  exit 1
fi

echo "[review] frozen surface markers"
rg 'cmd_livekit_tool_action' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs >/dev/null
rg 'assay\.receipt\.livekit\.tool_action\.v1' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs >/dev/null
rg 'livekit\.function-tools-executed\.export\.v1' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs >/dev/null
rg 'call_id mismatch' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs >/dev/null
rg 'completed.*false' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs >/dev/null
rg 'test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims' crates/assay-cli/tests/evidence_test.rs >/dev/null

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-cli
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -q -p assay-cli livekit_tool_action
cargo test -q -p assay-cli --test evidence_test test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims -- --exact
bash scripts/ci/review-week8-sota-gates.sh
git diff --check

echo "[review] PASS"
