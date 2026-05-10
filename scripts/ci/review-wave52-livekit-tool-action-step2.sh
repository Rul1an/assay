#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"

allowed_pattern='^(crates/assay-cli/src/cli/commands/evidence/livekit_tool_action\.rs|crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/(bundle|canonical|constants|input|reduce|tests|validate)\.rs|docs/contributing/SPLIT-CHECKLIST-wave52-livekit-tool-action-step2\.md|docs/contributing/SPLIT-MOVE-MAP-wave52-livekit-tool-action-step2\.md|docs/contributing/SPLIT-REVIEW-PACK-wave52-livekit-tool-action-step2\.md|scripts/ci/review-wave52-livekit-tool-action-step2\.sh)$'

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

echo "[review] Step2 allowlist vs $BASE_REF + local changes"
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
  echo "FAIL: Step2 must not touch workflows"
  exit 1
fi
if printf '%s\n' "$changed" | rg '^crates/assay-ebpf/src/vmlinux\.rs$' >/dev/null; then
  echo "FAIL: generated vmlinux.rs must stay out of scope"
  exit 1
fi
if printf '%s\n' "$changed" | rg 'receipt-schemas|docs/reference/receipt-schemas' >/dev/null; then
  echo "FAIL: Step2 must not edit schema files"
  exit 1
fi

echo "[review] split shape"
facade_loc="$(wc -l < crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs | tr -d ' ')"
if [ "$facade_loc" -gt 150 ]; then
  echo "FAIL: livekit_tool_action.rs facade is ${facade_loc} LOC; expected <= 150"
  exit 1
fi

for module in bundle canonical constants input reduce tests validate; do
  test -f "crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/${module}.rs"
done

echo "[review] frozen surface markers"
rg 'pub fn cmd_livekit_tool_action' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs >/dev/null
rg 'assay\.receipt\.livekit\.tool_action\.v1' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/constants.rs >/dev/null
rg 'livekit\.function-tools-executed\.export\.v1' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/constants.rs >/dev/null
rg 'call_id mismatch' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/reduce.rs >/dev/null
rg 'completed.*false' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/reduce.rs >/dev/null
rg 'capture context and session identity are out of scope' crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/constants.rs >/dev/null
rg 'test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims' crates/assay-cli/tests/evidence_test.rs >/dev/null

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-cli
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -q -p assay-cli livekit_tool_action
cargo test -q -p assay-cli --test evidence_test test_livekit_imported_tool_action_receipts_verify_and_do_not_mutate_trust_basis_claims -- --exact
git diff --check

echo "[review] PASS"
