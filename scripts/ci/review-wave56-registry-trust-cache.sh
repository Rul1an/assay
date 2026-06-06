#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REF="${BASE_REF:-origin/main}"
TRUST_FACADE="crates/assay-registry/src/trust.rs"
CACHE_FACADE="crates/assay-registry/src/cache.rs"
TRUST_NEXT_MOD="crates/assay-registry/src/trust_next/mod.rs"
CACHE_NEXT_MOD="crates/assay-registry/src/cache_next/mod.rs"
TRUST_TESTS="crates/assay-registry/src/trust_next/tests.rs"
CACHE_TESTS="crates/assay-registry/src/cache_next/tests.rs"

allowed_regex='^(crates/assay-registry/src/(trust\.rs|cache\.rs|trust_next/(mod|tests)\.rs|cache_next/(mod|tests)\.rs)|docs/contributing/SPLIT-(CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave56-registry-trust-cache\.md|scripts/ci/review-wave56-registry-trust-cache\.sh)$'

assert_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if ! rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

assert_not_rg() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg -n "$pattern" "$file" >/dev/null; then
    echo "FAIL: $message"
    exit 1
  fi
}

echo "[review] scope allowlist"
changed_files="$(
  {
    git diff --name-only "$BASE_REF" --
    git ls-files --others --exclude-standard -- \
      "$TRUST_FACADE" \
      "$CACHE_FACADE" \
      "$TRUST_NEXT_MOD" \
      "$CACHE_NEXT_MOD" \
      "$TRUST_TESTS" \
      "$CACHE_TESTS" \
      docs/contributing/SPLIT-CHECKLIST-wave56-registry-trust-cache.md \
      docs/contributing/SPLIT-MOVE-MAP-wave56-registry-trust-cache.md \
      docs/contributing/SPLIT-REVIEW-PACK-wave56-registry-trust-cache.md \
      scripts/ci/review-wave56-registry-trust-cache.sh
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

echo "[review] forbidden surfaces"
if ! git diff --quiet "$BASE_REF" -- Cargo.toml Cargo.lock .github/workflows; then
  echo "FAIL: Wave56 registry split must not touch Cargo files or workflows"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-registry/src/resolver.rs crates/assay-registry/tests/resolver_production_roots.rs; then
  echo "FAIL: Wave56 trust/cache split must not touch resolver behavior"
  exit 1
fi
if ! git diff --quiet "$BASE_REF" -- crates/assay-ebpf crates/assay-runner-core crates/assay-runner-linux crates/assay-runner-schema; then
  echo "FAIL: Wave56 registry split must not touch runner/eBPF crates"
  exit 1
fi

echo "[review] facade thinness"
trust_lines="$(wc -l < "$TRUST_FACADE" | tr -d ' ')"
cache_lines="$(wc -l < "$CACHE_FACADE" | tr -d ' ')"
echo "trust facade lines: $trust_lines"
echo "cache facade lines: $cache_lines"
if [ "$trust_lines" -gt 220 ]; then
  echo "FAIL: trust facade is too thick after split"
  exit 1
fi
if [ "$cache_lines" -gt 200 ]; then
  echo "FAIL: cache facade is too thick after split"
  exit 1
fi
assert_rg 'pub struct TrustStore' "$TRUST_FACADE" "TrustStore moved out of facade"
assert_rg 'pub struct KeyMetadata' "$TRUST_FACADE" "KeyMetadata moved out of facade"
assert_rg 'pub struct PackCache' "$CACHE_FACADE" "PackCache moved out of facade"
assert_rg 'pub struct CacheMeta' "$CACHE_FACADE" "CacheMeta moved out of facade"
assert_rg 'pub struct CacheEntry' "$CACHE_FACADE" "CacheEntry moved out of facade"
assert_not_rg 'mod tests \{' "$TRUST_FACADE" "inline trust tests still live in facade"
assert_not_rg 'mod tests \{' "$CACHE_FACADE" "inline cache tests still live in facade"

echo "[review] test boundary ownership"
assert_rg '^#\[cfg\(test\)\]$' "$TRUST_NEXT_MOD" "trust_next tests are not cfg-gated"
assert_rg '^mod tests;$' "$TRUST_NEXT_MOD" "trust_next tests module missing"
assert_rg '^#\[cfg\(test\)\]$' "$CACHE_NEXT_MOD" "cache_next tests are not cfg-gated"
assert_rg '^pub\(crate\) mod tests;$' "$CACHE_NEXT_MOD" "cache_next tests module missing"
assert_rg 'fn test_trust_rotation_pinned_root_survives_revocation' "$TRUST_TESTS" "pinned-root rotation test missing"
assert_rg 'fn test_key_id_mismatch_rejected' "$TRUST_TESTS" "key-id mismatch test missing"
assert_rg 'fn test_with_production_roots_loads_embedded_roots' "$TRUST_TESTS" "production root test missing"
assert_rg 'fn test_cache_integrity_failure' "$CACHE_TESTS" "cache integrity test missing"
assert_rg 'fn test_signature_json_corrupt_handling' "$CACHE_TESTS" "signature corruption test missing"
assert_rg 'fn test_atomic_write_prevents_partial_cache' "$CACHE_TESTS" "atomic write test missing"
assert_rg 'fn test_cache_registry_url_tracking' "$CACHE_TESTS" "registry URL tracking test missing"

echo "[review] repo checks"
cargo fmt --check
cargo check -p assay-registry
cargo test -p assay-registry trust_next::tests
cargo test -p assay-registry cache_next::tests
cargo clippy -p assay-registry --all-targets -- -D warnings
git diff --check "$BASE_REF" --

echo "[review] PASS"
