#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md"
  "docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step3.md"
  "scripts/ci/review-tr1-decision-emit-invariant-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/tests/decision_emit_invariant"
  "crates/assay-core/src"
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

for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: T-R1 Step3 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

PLAN="docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step3.md"

for marker in \
  'T-R1 Step2 shipped on `main` via `#980`' \
  'reviewer-script consistency follow-up shipped on `main` via `#981`' \
  'Step3 is the closure/docs+gates slice' \
  'tests/decision_emit_invariant/main.rs'
do
  rg -n -F "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'stable integration target root' \
  'fixtures.rs' \
  'emission.rs' \
  'approval.rs' \
  'restrict_scope.rs' \
  'redaction.rs' \
  'guard.rs' \
  'delegation.rs' \
  'g3_auth.rs' \
  'T-R1 is complete once this closure slice lands'
do
  rg -n -F "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings

echo "[review] PASS"
