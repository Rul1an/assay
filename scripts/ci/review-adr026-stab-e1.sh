#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-adapter-acp/src/lib.rs"
  "scripts/ci/review-adr026-stab-e1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 Stab E1 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 Stab E1: $f"
    exit 1
  fi
done

echo "[review] ACP attribute preservation markers"
rg -n 'payload.insert\("attributes"' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP payload must preserve attributes"
  exit 1
}
rg -n 'fn normalize_json' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP attributes must be normalized deterministically"
  exit 1
}
rg -n 'strict_attribute_order_normalizes_payload_but_keeps_raw_byte_hash_boundary' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: missing ACP normalization/hash-boundary test"
  exit 1
}

cargo test -p assay-adapter-acp >/dev/null

echo "[review] done"
