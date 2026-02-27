#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-stab-e0-b-impl}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-026-ATTACHMENT-WRITER-BOUNDARY.md"
  "scripts/ci/review-adr026-stab-e2-a.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E2A must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E2A: $f"
    exit 1
  fi
done

echo "[review] contract markers"
rg -n '^# ADR-026 AttachmentWriter Host Boundary \(E2A\)$' docs/architecture/ADR-026-ATTACHMENT-WRITER-BOUNDARY.md >/dev/null || {
  echo "FAIL: missing E2A title"
  exit 1
}
rg -n 'hard maximum payload size' docs/architecture/ADR-026-ATTACHMENT-WRITER-BOUNDARY.md >/dev/null || {
  echo "FAIL: missing size-cap contract"
  exit 1
}
rg -n 'media-type validation' docs/architecture/ADR-026-ATTACHMENT-WRITER-BOUNDARY.md >/dev/null || {
  echo "FAIL: missing media-type contract"
  exit 1
}
rg -n '^## Redaction boundary$' docs/architecture/ADR-026-ATTACHMENT-WRITER-BOUNDARY.md >/dev/null || {
  echo "FAIL: missing redaction boundary section"
  exit 1
}
rg -n '^## Error taxonomy$' docs/architecture/ADR-026-ATTACHMENT-WRITER-BOUNDARY.md >/dev/null || {
  echo "FAIL: missing error taxonomy section"
  exit 1
}

echo "[review] done"
