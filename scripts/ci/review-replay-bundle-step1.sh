#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave17-replay-bundle.md"
  "docs/contributing/SPLIT-CHECKLIST-replay-bundle-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-replay-bundle-step1.md"
  "scripts/ci/review-replay-bundle-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, replay ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave17 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave17 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^crates/assay-core/src/replay/' >/dev/null; then
  echo "FAIL: Wave17 Step1 must not change crates/assay-core/src/replay/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/src/replay/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/src/replay/** are not allowed in Wave17 Step1"
  git ls-files --others --exclude-standard -- 'crates/assay-core/src/replay/**' | sed 's/^/  - /'
  exit 1
fi

cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

cargo test -p assay-core --lib replay::bundle::tests::write_bundle_minimal_roundtrip -- --exact
cargo test -p assay-core --lib replay::bundle::tests::bundle_digest_equals_sha256_of_written_bytes -- --exact
cargo test -p assay-core --lib replay::verify::tests::verify_clean_bundle_passes -- --exact

echo "[review] PASS"
