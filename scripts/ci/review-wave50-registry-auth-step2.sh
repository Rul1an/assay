#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-registry/src/auth.rs"
  "crates/assay-registry/src/auth_next/mod.rs"
  "crates/assay-registry/src/auth_next/providers.rs"
  "crates/assay-registry/src/auth_next/oidc.rs"
  "crates/assay-registry/src/auth_next/cache.rs"
  "crates/assay-registry/src/auth_next/headers.rs"
  "crates/assay-registry/src/auth_next/diagnostics.rs"
  "docs/contributing/SPLIT-PLAN-wave50-registry-auth.md"
  "docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step2.md"
  "scripts/ci/review-wave50-registry-auth-step2.sh"
)

RUST_SCOPE_FILES=(
  "crates/assay-registry/src/auth.rs"
  "crates/assay-registry/src/auth_next/mod.rs"
  "crates/assay-registry/src/auth_next/providers.rs"
  "crates/assay-registry/src/auth_next/oidc.rs"
  "crates/assay-registry/src/auth_next/cache.rs"
  "crates/assay-registry/src/auth_next/headers.rs"
  "crates/assay-registry/src/auth_next/diagnostics.rs"
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
    echo "FAIL: Wave50 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave50 Step2: $f"
    exit 1
  fi
done < <(changed_files | awk 'NF' | sort -u)

echo "[review] registry tests must remain untouched"
if git diff --name-only "$BASE_REF"...HEAD -- "crates/assay-registry/tests" | rg -n '.' >/dev/null; then
  echo "FAIL: registry integration tests changed in Wave50 Step2"
  git diff --name-only "$BASE_REF"...HEAD -- "crates/assay-registry/tests"
  exit 1
fi

echo "[review] non-auth registry source must remain untouched"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue
  if [[ "$f" == crates/assay-registry/src/* ]] && \
     [[ "$f" != crates/assay-registry/src/auth.rs ]] && \
     [[ "$f" != crates/assay-registry/src/auth_next/* ]]; then
    echo "FAIL: non-auth registry source changed out of scope: $f"
    exit 1
  fi
done < <(changed_files | awk 'NF' | sort -u)

echo "[review] marker and boundary checks"
PLAN="docs/contributing/SPLIT-PLAN-wave50-registry-auth.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step2.md"

for marker in \
  'Wave50 Step1 shipped on `main` via `#986`.' \
  '`auth.rs`: stable facade, public `TokenProvider` / `OidcProvider`, private cache structs, and existing inline tests' \
  'no static/env precedence drift' \
  'no OIDC exchange or request-header drift' \
  'no retry/backoff drift'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'auth_next/providers.rs' \
  'auth_next/oidc.rs' \
  'auth_next/cache.rs' \
  'auth_next/headers.rs' \
  'auth_next/diagnostics.rs' \
  'identical downstream auth-header and unauthorized-response behavior in the registry client'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

if ! rg -F -n '#[path = "auth_next/mod.rs"]' crates/assay-registry/src/auth.rs >/dev/null; then
  echo 'FAIL: auth.rs must wire auth_next/mod.rs via #[path = "auth_next/mod.rs"]'
  exit 1
fi

if rg -n 'reqwest::Client::new|tokio::time::sleep|\.post\(&self\.registry_exchange_url\)|\.header\("Authorization"|tracing::warn!|tracing::info!' \
  crates/assay-registry/src/auth.rs >/dev/null; then
  echo "FAIL: auth.rs facade still contains provider/OIDC implementation details"
  exit 1
fi

rg -n 'ASSAY_REGISTRY_TOKEN|ASSAY_REGISTRY_OIDC' crates/assay-registry/src/auth_next/providers.rs >/dev/null || {
  echo "FAIL: providers.rs must own env precedence"
  exit 1
}

rg -n 'ACTIONS_ID_TOKEN_REQUEST_URL|ACTIONS_ID_TOKEN_REQUEST_TOKEN|reqwest::Client::new|exchange_token_with_retry|get_github_oidc_token|exchange_for_registry_token' \
  crates/assay-registry/src/auth_next/oidc.rs >/dev/null || {
  echo "FAIL: oidc.rs must own exchange and request logic"
  exit 1
}

rg -n 'cached_token\.read|cached_token\.write' crates/assay-registry/src/auth_next/cache.rs >/dev/null || {
  echo "FAIL: cache.rs must own cached token access"
  exit 1
}

rg -n 'api-version=2\.0|Bearer \{\}' crates/assay-registry/src/auth_next/headers.rs >/dev/null || {
  echo "FAIL: headers.rs must own OIDC header helpers"
  exit 1
}

count_base_matches() {
  local pattern="$1"
  local total=0
  local count
  for file in "${RUST_SCOPE_FILES[@]}"; do
    if git cat-file -e "$BASE_REF:$file" 2>/dev/null; then
      count=$(git show "$BASE_REF:$file" | rg -o "$pattern" | wc -l | tr -d ' ' || true)
      total=$((total + count))
    fi
  done
  echo "$total"
}

count_head_matches() {
  local pattern="$1"
  local total=0
  local count
  for file in "${RUST_SCOPE_FILES[@]}"; do
    if [[ -f "$file" ]]; then
      count=$(rg -o "$pattern" "$file" | wc -l | tr -d ' ' || true)
      total=$((total + count))
    fi
  done
  echo "$total"
}

for pattern in 'unwrap\(' 'expect\(' '\bunsafe\b' 'println!\(' 'eprintln!\(' 'panic!\(' 'todo!\(' 'unimplemented!\('; do
  base_count="$(count_base_matches "$pattern")"
  head_count="$(count_head_matches "$pattern")"
  if (( head_count > base_count )); then
    echo "FAIL: pattern '$pattern' increased in Wave50 Step2 scope: $base_count -> $head_count"
    exit 1
  fi
done

echo "[review] repo checks"
cargo fmt --all --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo check -q -p assay-registry
cargo check -q -p assay-registry --features oidc

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
