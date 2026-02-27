#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-adapter-api/src/lib.rs"
  "crates/assay-adapter-acp/src/lib.rs"
  "crates/assay-adapter-a2a/src/lib.rs"
  "scripts/ci/review-adr026-stab-e0-b.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E0B must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E0B: $f"
    exit 1
  fi
done

echo "[review] adapter metadata markers"
rg -n 'pub struct AdapterDescriptor' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: AdapterDescriptor missing"
  exit 1
}
rg -n 'fn adapter\(&self\) -> AdapterDescriptor' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: ProtocolAdapter::adapter contract missing"
  exit 1
}
rg -n '"adapter_id"' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP adapter_id metadata missing"
  exit 1
}
rg -n '"adapter_version"' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: ACP adapter_version metadata missing"
  exit 1
}
rg -n '"adapter_id"' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A adapter_id metadata missing"
  exit 1
}
rg -n '"adapter_version"' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: A2A adapter_version metadata missing"
  exit 1
}

cargo test -p assay-adapter-api -p assay-adapter-acp -p assay-adapter-a2a >/dev/null

echo "[review] done"
