#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave48-registry-trust.md"
  "docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step3.md"
  "scripts/ci/review-wave48-registry-trust-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-registry/src"
  "crates/assay-registry/tests"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave48 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave48 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave48 Step3 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-wave48-registry-trust.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step3.md"

for marker in \
  'Wave48 Step2 shipped on `main` via `#970`.' \
  'keep `trust.rs` as the stable facade entrypoint' \
  'Step3 constraints:' \
  'no new module cuts' \
  'no behavior cleanup beyond internal follow-up notes'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-registry/src/trust.rs' \
  'crates/assay-registry/src/trust_next/manifest.rs' \
  'crates/assay-registry/src/trust_next/access.rs' \
  'future internal visibility tightening only if it requires a separate code wave' \
  'registry contract or public surface changes'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings

echo "[review] pinned trust invariants"
cargo test -q -p assay-registry --lib 'trust::tests::test_with_production_roots_loads_embedded_roots' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_add_from_manifest' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_pinned_key_not_overwritten' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_needs_refresh' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_revoke_old_key' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_pinned_root_survives_revocation' -- --exact
cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_key_expires_after_added' -- --exact
cargo test -q -p assay-registry --test resolver_production_roots resolver_accepts_signed_pack_with_embedded_production_root -- --exact
cargo test -q -p assay-registry --test resolver_production_roots resolver_rejects_signed_pack_with_untrusted_key_id -- --exact

echo "[review] PASS"
