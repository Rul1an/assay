#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
TEST_FILE="crates/assay-registry/tests/resolver_contracts.rs"

allowed_regex='^(crates/assay-registry/tests/resolver_contracts\.rs|docs/contributing/SPLIT-(PLAN-wave57-registry-resolver|CHECKLIST-wave57-registry-resolver-contracts|REVIEW-PACK-wave57-registry-resolver-contracts)\.md|scripts/ci/review-wave57-registry-resolver-contracts\.sh)$'

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

echo "[review] scope allowlist"
changed_files="$(
  {
    git diff --name-only "$BASE_REF" --
    git ls-files --others --exclude-standard -- \
      "$TEST_FILE" \
      docs/contributing/SPLIT-PLAN-wave57-registry-resolver.md \
      docs/contributing/SPLIT-CHECKLIST-wave57-registry-resolver-contracts.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave57-registry-resolver-contracts.md \
      scripts/ci/review-wave57-registry-resolver-contracts.sh
  } | sort -u
)"
while IFS= read -r file; do
  [ -n "$file" ] || continue
  if [[ ! "$file" =~ $allowed_regex ]]; then
    echo "FAIL: out-of-scope path changed: $file"
    exit 1
  fi
done <<EOF
$changed_files
EOF

echo "[review] forbidden runtime surfaces"
if ! git diff --quiet "$BASE_REF" -- crates/assay-registry/src/resolver.rs crates/assay-registry/src/cache.rs crates/assay-registry/src/trust.rs crates/assay-registry/src/client crates/assay-registry/src/verify.rs; then
  echo "FAIL: Wave57 resolver contracts must not edit runtime resolver/cache/trust/client/verify code"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- Cargo.toml Cargo.lock .github/workflows; then
  echo "FAIL: Wave57 resolver contracts must not touch Cargo files or workflows"
  exit 1
fi

echo "[review] contract coverage"
assert_rg 'async fn resolver_uses_fresh_cache_before_network' "$TEST_FILE" "cache-first resolver contract missing"
assert_rg 'async fn resolver_evicts_pinned_cache_mismatch_and_refetches' "$TEST_FILE" "pinned digest refetch contract missing"
assert_rg 'async fn resolver_no_cache_skips_cached_entry_and_fetches_registry' "$TEST_FILE" "no_cache registry fetch contract missing"
assert_rg 'PackResolver::with_components' "$TEST_FILE" "resolver contracts must use public component injection"
assert_rg 'ResolveSource::Cache' "$TEST_FILE" "cache source assertion missing"
assert_rg 'ResolveSource::Registry' "$TEST_FILE" "registry source assertion missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-registry
cargo test -p assay-registry resolver
cargo test -p assay-registry --test resolver_contracts
cargo clippy -p assay-registry --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
