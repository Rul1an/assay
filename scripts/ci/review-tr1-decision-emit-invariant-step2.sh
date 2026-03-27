#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "crates/assay-core/tests/decision_emit_invariant.rs"
  "crates/assay-core/tests/decision_emit_invariant/main.rs"
  "crates/assay-core/tests/decision_emit_invariant/fixtures.rs"
  "crates/assay-core/tests/decision_emit_invariant/emission.rs"
  "crates/assay-core/tests/decision_emit_invariant/approval.rs"
  "crates/assay-core/tests/decision_emit_invariant/restrict_scope.rs"
  "crates/assay-core/tests/decision_emit_invariant/redaction.rs"
  "crates/assay-core/tests/decision_emit_invariant/guard.rs"
  "crates/assay-core/tests/decision_emit_invariant/delegation.rs"
  "crates/assay-core/tests/decision_emit_invariant/g3_auth.rs"
  "docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md"
  "docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step2.md"
  "scripts/ci/review-tr1-decision-emit-invariant-step2.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src"
)

if ! git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
  echo "BASE_REF does not resolve to a commit: $BASE_REF" >&2
  exit 1
fi

tmp_changed="$(mktemp)"
tmp_list="$(mktemp)"
trap 'rm -f "$tmp_changed" "$tmp_list"' EXIT

git diff --name-only "$BASE_REF"...HEAD >"$tmp_changed"
git diff --name-only >>"$tmp_changed"
git diff --name-only --cached >>"$tmp_changed"
git ls-files --others --exclude-standard >>"$tmp_changed"

sort -u -o "$tmp_changed" "$tmp_changed"

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r file; do
  [[ -z "$file" ]] && continue

  if [[ "$file" == .github/workflows/* ]]; then
    echo "workflow file changed out of scope: $file" >&2
    exit 1
  fi

  allowed=false
  for allowed_file in "${ALLOWED_FILES[@]}"; do
    if [[ "$file" == "$allowed_file" ]]; then
      allowed=true
      break
    fi
  done

  if [[ "$allowed" == false ]]; then
    echo "out-of-scope file changed: $file" >&2
    exit 1
  fi
done <"$tmp_changed"

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: T-R1 Step2 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

if [[ -f crates/assay-core/tests/decision_emit_invariant.rs ]]; then
  echo "FAIL: single-file target should have been converted to tests/decision_emit_invariant/main.rs"
  exit 1
fi

if [[ ! -f crates/assay-core/tests/decision_emit_invariant/main.rs ]]; then
  echo "FAIL: target root missing: crates/assay-core/tests/decision_emit_invariant/main.rs"
  exit 1
fi

if rg -n '^#\\[test\\]' crates/assay-core/tests/decision_emit_invariant/main.rs >/dev/null; then
  echo "FAIL: main.rs must remain module wiring only"
  exit 1
fi

if rg -n '^#\\[test\\]' crates/assay-core/tests/decision_emit_invariant/fixtures.rs >/dev/null; then
  echo "FAIL: fixtures.rs must remain helper-only"
  exit 1
fi

PLAN="docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step2.md"

for marker in \
  'tests/decision_emit_invariant/main.rs' \
  'module wiring only' \
  'fixtures.rs' \
  'one integration-test binary' \
  'do not introduce a second integration-test target'
do
  rg -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-core/tests/decision_emit_invariant.rs' \
  'crates/assay-core/tests/decision_emit_invariant/main.rs' \
  'test_policy_allow_emits_once' \
  'approval_required_missing_denies' \
  'test_delegation_fields_are_additive_on_emitted_decisions' \
  'g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json'
do
  rg -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings

cargo test -q -p assay-core --test decision_emit_invariant -- --list >"$tmp_list"

for selector in \
  'emission::test_policy_allow_emits_once: test' \
  'delegation::test_delegation_fields_are_additive_on_emitted_decisions: test' \
  'approval::approval_required_missing_denies: test' \
  'restrict_scope::restrict_scope_target_missing_denies: test' \
  'redaction::redact_args_target_missing_denies: test' \
  'guard::test_guard_emits_on_panic: test' \
  'emission::test_event_contains_required_fields: test' \
  'g3_auth::g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json: test'
do
  rg -n "^${selector}$" "$tmp_list" >/dev/null || {
    echo "FAIL: missing selector in decision_emit_invariant target: $selector"
    exit 1
  }
done

cargo test -q -p assay-core --test decision_emit_invariant emission::test_policy_allow_emits_once -- --exact
cargo test -q -p assay-core --test decision_emit_invariant delegation::test_delegation_fields_are_additive_on_emitted_decisions -- --exact
cargo test -q -p assay-core --test decision_emit_invariant approval::approval_required_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant restrict_scope::restrict_scope_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant redaction::redact_args_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant guard::test_guard_emits_on_panic -- --exact
cargo test -q -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -q -p assay-core --test decision_emit_invariant g3_auth::g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json -- --exact
cargo test -q -p assay-core --test decision_emit_invariant

echo "[review] PASS"
