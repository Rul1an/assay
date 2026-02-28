#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.lock"
  "crates/assay-adapter-api/src/lib.rs"
  "crates/assay-adapter-api/src/shape.rs"
  "crates/assay-adapter-acp/Cargo.toml"
  "crates/assay-adapter-acp/src/lib.rs"
  "crates/assay-adapter-a2a/Cargo.toml"
  "crates/assay-adapter-a2a/src/lib.rs"
  "scripts/ci/review-adr026-stab-e4-b.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E4B must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E4B: $f"
    exit 1
  fi
done

echo "[review] parser hardening markers"
rg -n 'pub fn validate_json_shape' crates/assay-adapter-api/src/shape.rs >/dev/null || {
  echo "FAIL: shared validate_json_shape helper missing"
  exit 1
}
rg -n 'max_json_depth' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: ConvertOptions max_json_depth missing"
  exit 1
}
rg -n 'max_array_length' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: ConvertOptions max_array_length missing"
  exit 1
}
rg -n 'validate_json_shape' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP shape validation missing"
  exit 1
}
rg -n 'validate_json_shape' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A shape validation missing"
  exit 1
}
rg -n 'invalid UTF-8' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP invalid UTF-8 test coverage missing"
  exit 1
}
rg -n 'invalid UTF-8' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A invalid UTF-8 test coverage missing"
  exit 1
}
rg -n 'proptest!' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP property test missing"
  exit 1
}
rg -n 'proptest!' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A property test missing"
  exit 1
}

cargo test -p assay-adapter-api -p assay-adapter-acp -p assay-adapter-a2a >/dev/null

echo "[review] done"
