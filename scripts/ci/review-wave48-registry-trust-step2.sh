#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "crates/assay-registry/src/trust.rs"
  "crates/assay-registry/src/trust_next/mod.rs"
  "crates/assay-registry/src/trust_next/decode.rs"
  "crates/assay-registry/src/trust_next/pinned.rs"
  "crates/assay-registry/src/trust_next/manifest.rs"
  "crates/assay-registry/src/trust_next/cache.rs"
  "crates/assay-registry/src/trust_next/access.rs"
  "docs/contributing/SPLIT-PLAN-wave48-registry-trust.md"
  "docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step2.md"
  "scripts/ci/review-wave48-registry-trust-step2.sh"
)

DIFF_FILES=()
while IFS= read -r file; do
  DIFF_FILES+=("$file")
done < <(git diff --name-only "$BASE_REF"...HEAD)
while IFS= read -r file; do
  DIFF_FILES+=("$file")
done < <(git ls-files --others --exclude-standard)

if (( ${#DIFF_FILES[@]} > 0 )); then
  for file in "${DIFF_FILES[@]}"; do
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
  done
fi

if (( ${#DIFF_FILES[@]} > 0 )); then
  for file in "${DIFF_FILES[@]}"; do
    [[ -z "$file" ]] && continue
    if [[ "$file" == crates/assay-registry/tests/* ]]; then
      echo "registry tests must remain untouched in Step2" >&2
      exit 1
    fi
    if [[ "$file" == crates/assay-registry/src/resolver.rs ]]; then
      echo "resolver.rs must remain untouched in Wave48 Step2" >&2
      exit 1
    fi
    if [[ "$file" == crates/assay-registry/src/verify.rs ]]; then
      echo "verify.rs must remain untouched in Wave48 Step2" >&2
      exit 1
    fi
  done
fi

if ! rg -n '^#\[path = "trust_next/mod.rs"\]$' crates/assay-registry/src/trust.rs >/dev/null; then
  echo "trust.rs must declare the sibling trust_next path override" >&2
  exit 1
fi

if ! rg -n '^mod trust_next;$' crates/assay-registry/src/trust.rs >/dev/null; then
  echo "trust.rs must declare trust_next module" >&2
  exit 1
fi

for forbidden in \
  '^fn decode_verifying_key\(' \
  '^fn decode_public_key_bytes\(' \
  '^fn parse_pinned_roots_json_impl\(' \
  '^fn load_production_roots_impl\(' \
  '^fn insert_pinned_key\(' \
  '^fn get_key_inner\(' \
  '^fn empty_inner\('
do
  if rg -n "$forbidden" crates/assay-registry/src/trust.rs >/dev/null; then
    echo "trust.rs still contains extracted implementation symbol: $forbidden" >&2
    exit 1
  fi
done

RUST_SCOPE_FILES=(
  "crates/assay-registry/src/trust.rs"
  "crates/assay-registry/src/trust_next/mod.rs"
  "crates/assay-registry/src/trust_next/decode.rs"
  "crates/assay-registry/src/trust_next/pinned.rs"
  "crates/assay-registry/src/trust_next/manifest.rs"
  "crates/assay-registry/src/trust_next/cache.rs"
  "crates/assay-registry/src/trust_next/access.rs"
)

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
    echo "pattern '$pattern' increased in trust split scope: $base_count -> $head_count" >&2
    exit 1
  fi
done

cargo fmt --all --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo check -q -p assay-registry

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
