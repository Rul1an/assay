#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-stab-e1-acp-lossiness}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-026-ADAPTER-METADATA-CONTRACT.md"
  "scripts/ci/review-adr026-stab-e0-a.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E0A must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E0A: $f"
    exit 1
  fi
done

echo "[review] contract markers"
rg -n '^# ADR-026 Adapter Metadata Contract \(E0\)$' docs/architecture/ADR-026-ADAPTER-METADATA-CONTRACT.md >/dev/null || {
  echo "FAIL: missing E0 contract title"
  exit 1
}
rg -n '`adapter_id`' docs/architecture/ADR-026-ADAPTER-METADATA-CONTRACT.md >/dev/null || {
  echo "FAIL: adapter_id requirement missing"
  exit 1
}
rg -n '`adapter_version`' docs/architecture/ADR-026-ADAPTER-METADATA-CONTRACT.md >/dev/null || {
  echo "FAIL: adapter_version requirement missing"
  exit 1
}

echo "[review] done"
