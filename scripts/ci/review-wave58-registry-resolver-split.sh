#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
FACADE="crates/assay-registry/src/resolver.rs"
IMPL_DIR="crates/assay-registry/src/resolver_next"

allowed_regex='^(crates/assay-registry/src/resolver\.rs|crates/assay-registry/src/resolver_next/(mod|local|bundled|registry|byos|tests)\.rs|docs/contributing/SPLIT-(PLAN|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave58-registry-resolver-split\.md|scripts/ci/review-wave58-registry-resolver-split\.sh)$'

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
      "$FACADE" \
      "$IMPL_DIR" \
      docs/contributing/SPLIT-PLAN-wave58-registry-resolver-split.md \
      docs/contributing/SPLIT-CHECKLIST-wave58-registry-resolver-split.md \
      docs/contributing/SPLIT-MOVE-MAP-wave58-registry-resolver-split.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave58-registry-resolver-split.md \
      scripts/ci/review-wave58-registry-resolver-split.sh
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

echo "[review] forbidden drift"
if ! git diff --quiet "$BASE_REF" -- Cargo.toml Cargo.lock .github/workflows; then
  echo "FAIL: Wave58 resolver split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-registry/src/cache.rs crates/assay-registry/src/trust.rs crates/assay-registry/src/client crates/assay-registry/src/verify.rs crates/assay-registry/src/lockfile.rs; then
  echo "FAIL: Wave58 resolver split must not touch adjacent registry runtime files"
  exit 1
fi

echo "[review] facade shape"
facade_loc="$(wc -l < "$FACADE" | tr -d ' ')"
if [ "$facade_loc" -gt 40 ]; then
  echo "FAIL: resolver facade grew too large: $facade_loc LOC"
  exit 1
fi
assert_rg '#\[path = "resolver_next/mod.rs"\]' "$FACADE" "resolver facade must route to resolver_next"
assert_rg 'pub use resolver_next::\{PackResolver, ResolveSource, ResolvedPack, ResolverConfig\};' "$FACADE" "resolver facade must preserve public re-exports"
if rg -n 'async fn resolve_(local|bundled|registry|byos)|fn try_cache|pub struct PackResolver|pub struct ResolverConfig|pub enum ResolveSource' "$FACADE" >/dev/null; then
  echo "FAIL: resolver facade still owns moved implementation"
  exit 1
fi

echo "[review] module ownership"
assert_rg '^mod bundled;' "$IMPL_DIR/mod.rs" "bundled module declaration missing"
assert_rg '^mod byos;' "$IMPL_DIR/mod.rs" "byos module declaration missing"
assert_rg '^mod local;' "$IMPL_DIR/mod.rs" "local module declaration missing"
assert_rg '^mod registry;' "$IMPL_DIR/mod.rs" "registry module declaration missing"
assert_rg 'pub struct PackResolver' "$IMPL_DIR/mod.rs" "PackResolver must live in resolver_next/mod.rs"
assert_rg 'pub\(super\) async fn resolve_local' "$IMPL_DIR/local.rs" "local resolver method must live in local.rs"
assert_rg 'pub\(super\) async fn resolve_bundled' "$IMPL_DIR/bundled.rs" "bundled resolver method must live in bundled.rs"
assert_rg 'pub\(super\) async fn resolve_registry' "$IMPL_DIR/registry.rs" "registry resolver method must live in registry.rs"
assert_rg 'async fn try_cache' "$IMPL_DIR/registry.rs" "cache helper must stay private in registry.rs"
assert_rg 'pub\(super\) async fn resolve_byos' "$IMPL_DIR/byos.rs" "BYOS resolver method must live in byos.rs"
if rg -n 'pub\(crate\)|pub ' "$IMPL_DIR"/{local,bundled,registry,byos}.rs >/dev/null; then
  echo "FAIL: route modules must not expose new crate/public API"
  exit 1
fi

echo "[review] contract coverage"
assert_rg 'async fn resolver_uses_fresh_cache_before_network' crates/assay-registry/tests/resolver_contracts.rs "cache-first resolver contract missing"
assert_rg 'async fn resolver_evicts_pinned_cache_mismatch_and_refetches' crates/assay-registry/tests/resolver_contracts.rs "pinned digest refetch contract missing"
assert_rg 'async fn resolver_no_cache_skips_cached_entry_and_fetches_registry' crates/assay-registry/tests/resolver_contracts.rs "no_cache registry fetch contract missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-registry
cargo test -p assay-registry resolver
cargo test -p assay-registry --test resolver_contracts
cargo clippy -p assay-registry --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
