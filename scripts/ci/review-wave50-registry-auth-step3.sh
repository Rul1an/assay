#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave50-registry-auth.md"
  "docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step3.md"
  "scripts/ci/review-wave50-registry-auth-step3.sh"
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
    echo "FAIL: Wave50 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave50 Step3: $f"
    exit 1
  fi
done < <(changed_files | awk 'NF' | sort -u)

echo "[review] auth source and tests must remain untouched"
if git diff --name-only "$BASE_REF"...HEAD -- "crates/assay-registry/src" | rg -n '.' >/dev/null; then
  echo "FAIL: registry source changed in Wave50 Step3"
  git diff --name-only "$BASE_REF"...HEAD -- "crates/assay-registry/src"
  exit 1
fi

if git diff --name-only "$BASE_REF"...HEAD -- "crates/assay-registry/tests" | rg -n '.' >/dev/null; then
  echo "FAIL: registry tests changed in Wave50 Step3"
  git diff --name-only "$BASE_REF"...HEAD -- "crates/assay-registry/tests"
  exit 1
fi

if git status --porcelain -- crates/assay-registry/src crates/assay-registry/tests | rg -n '^\?\?' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-registry/src/** or tests/** are forbidden in Step3"
  git status --porcelain -- crates/assay-registry/src crates/assay-registry/tests
  exit 1
fi

echo "[review] closure markers"
rg -F -n 'Wave50 Step2 shipped on `main` via `#987`.' docs/contributing/SPLIT-PLAN-wave50-registry-auth.md >/dev/null || {
  echo "FAIL: plan must record Step2 landing on main"
  exit 1
}
rg -F -n 'Wave50 Step3 is the closure/docs+gates-only slice for the shipped auth split.' docs/contributing/SPLIT-PLAN-wave50-registry-auth.md >/dev/null || {
  echo "FAIL: plan must describe Step3 as closure/docs+gates only"
  exit 1
}
rg -F -n 'Step3 is a closure slice. No Rust bodies move in this step.' docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step3.md >/dev/null || {
  echo "FAIL: Step3 move-map must assert no Rust-body moves"
  exit 1
}

echo "[review] quality checks"
cargo fmt --all --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo check -q -p assay-registry
cargo check -q -p assay-registry --features oidc

echo "[review] auth invariants"
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

echo "[review] facade invariants"
rg -F -n '#[path = "auth_next/mod.rs"]' crates/assay-registry/src/auth.rs >/dev/null || {
  echo 'FAIL: auth.rs must continue wiring auth_next/mod.rs'
  exit 1
}

auth_loc="$(awk 'NF{c++} END{print c+0}' crates/assay-registry/src/auth.rs)"
if [ "$auth_loc" -gt 540 ]; then
  echo "FAIL: auth.rs facade LOC budget exceeded ($auth_loc > 540)"
  exit 1
fi

rg -n 'ASSAY_REGISTRY_TOKEN|ASSAY_REGISTRY_OIDC' crates/assay-registry/src/auth_next/providers.rs >/dev/null || {
  echo "FAIL: providers.rs must continue owning env precedence"
  exit 1
}
rg -n 'ACTIONS_ID_TOKEN_REQUEST_URL|ACTIONS_ID_TOKEN_REQUEST_TOKEN|exchange_token_with_retry|get_github_oidc_token|exchange_for_registry_token' crates/assay-registry/src/auth_next/oidc.rs >/dev/null || {
  echo "FAIL: oidc.rs must continue owning exchange/request flow"
  exit 1
}
rg -n 'cached_token\.read|cached_token\.write' crates/assay-registry/src/auth_next/cache.rs >/dev/null || {
  echo "FAIL: cache.rs must continue owning cached token access"
  exit 1
}
rg -n 'api-version=2\.0|Bearer \{\}' crates/assay-registry/src/auth_next/headers.rs >/dev/null || {
  echo "FAIL: headers.rs must continue owning OIDC header helpers"
  exit 1
}

echo "[review] PASS"
