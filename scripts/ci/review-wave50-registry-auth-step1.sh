#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave50-registry-auth.md"
  "docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step1.md"
  "scripts/ci/review-wave50-registry-auth-step1.sh"
)

FROZEN_PATHS=(
  "crates/assay-registry/src"
  "crates/assay-registry/tests"
)

changed_files() {
  git diff --name-only "$BASE_REF"...HEAD || true
  git diff --name-only || true
  git diff --name-only --cached || true
  git ls-files --others --exclude-standard || true
}

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave50 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave50 Step1: $f"
    exit 1
  fi
done < <(changed_files | awk 'NF' | sort -u)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave50 Step1 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-wave50-registry-auth.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step1.md"

for marker in \
  'Split `crates/assay-registry/src/auth.rs` behind a stable facade' \
  '`TokenProvider::from_env`' \
  '`OidcProvider::get_token`' \
  'no static/env precedence drift' \
  'no downstream auth-header or unauthorized-response drift'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-registry/src/auth_next/providers.rs' \
  'crates/assay-registry/src/auth_next/oidc.rs' \
  'identical retry/backoff behavior' \
  'registry_client::scenarios_auth_headers::test_authentication_header'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings

echo "[review] pinned auth invariants"
cargo test -q -p assay-registry --lib 'auth::tests::test_static_token' -- --exact
cargo test -q -p assay-registry --lib 'auth::tests::test_from_env_static' -- --exact
cargo test -q -p assay-registry --lib 'auth::tests::test_from_env_empty_token' -- --exact
cargo test -q -p assay-registry --lib 'auth::tests::test_get_static_token' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_full_flow' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_github_failure' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_cache_clear' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_token_expiry_triggers_refresh' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_retry_backoff_on_failure' -- --exact
cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_auth_headers::test_authentication_header' -- --exact
cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_auth_headers::test_no_auth_when_no_token' -- --exact
cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_pack_fetch::test_fetch_pack_unauthorized' -- --exact

echo "[review] PASS"
