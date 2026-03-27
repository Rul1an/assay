#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md"
  "docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step1.md"
  "scripts/ci/review-tr1-decision-emit-invariant-step1.sh"
)

if ! git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
  echo "BASE_REF does not resolve to a commit: $BASE_REF" >&2
  exit 1
fi

tmp_changed="$(mktemp)"
trap 'rm -f "$tmp_changed"' EXIT

git diff --name-only "$BASE_REF"...HEAD >"$tmp_changed"
git diff --name-only >>"$tmp_changed"
git diff --name-only --cached >>"$tmp_changed"
git ls-files --others --exclude-standard >>"$tmp_changed"

sort -u -o "$tmp_changed" "$tmp_changed"

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

while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  if [[ "$file" == crates/assay-core/tests/* ]]; then
    echo "assay-core integration tests must remain untouched in T-R1 Step1" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/src/mcp/* ]]; then
    echo "assay-core mcp sources must remain untouched in T-R1 Step1" >&2
    exit 1
  fi
done <"$tmp_changed"

cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings

cargo test -q -p assay-core --test decision_emit_invariant test_policy_allow_emits_once -- --exact
cargo test -q -p assay-core --test decision_emit_invariant test_delegation_fields_are_additive_on_emitted_decisions -- --exact
cargo test -q -p assay-core --test decision_emit_invariant approval_required_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant restrict_scope_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant redact_args_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant test_guard_emits_on_panic -- --exact
cargo test -q -p assay-core --test decision_emit_invariant test_event_contains_required_fields -- --exact
cargo test -q -p assay-core --test decision_emit_invariant g3_auth_projection_emits_allowlisted_scheme_trimmed_issuer_principal_in_decision_json -- --exact

echo "[review] PASS"
