#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-stab-e2-b-host-writer}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.lock"
  "crates/assay-adapter-api/Cargo.toml"
  "crates/assay-adapter-api/src/canonical.rs"
  "crates/assay-adapter-api/src/lib.rs"
  "crates/assay-adapter-acp/src/lib.rs"
  "crates/assay-adapter-a2a/src/lib.rs"
  "scripts/ci/review-adr026-stab-e3-b.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E3B must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E3B: $f"
    exit 1
  fi
done

echo "[review] canonicalization markers"
rg -n 'pub fn canonical_json_bytes' crates/assay-adapter-api/src/canonical.rs >/dev/null || {
  echo "FAIL: canonical_json_bytes missing"
  exit 1
}
rg -n 'pub fn digest_canonical_json' crates/assay-adapter-api/src/canonical.rs >/dev/null || {
  echo "FAIL: digest_canonical_json missing"
  exit 1
}
rg -n 'digest_canonical_json' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP canonical digest adoption missing"
  exit 1
}
rg -n 'digest_canonical_json' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A canonical digest adoption missing"
  exit 1
}
rg -n 'raw_payload_ref' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP raw payload boundary coverage missing"
  exit 1
}
rg -n 'raw_payload_ref' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A raw payload boundary coverage missing"
  exit 1
}

cargo test -p assay-adapter-api -p assay-adapter-acp -p assay-adapter-a2a >/dev/null

echo "[review] done"
