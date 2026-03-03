#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/exp-mcp-fragmented-ipi-live-hardening}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/exp-mcp-fragmented-ipi/compat_host/"
  "scripts/ci/test-exp-mcp-fragmented-ipi-compat-host.sh"
  "scripts/ci/review-exp-mcp-fragmented-ipi-compat-host-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: compat-host Step2 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && ok="true"
    else
      [[ "$f" == "$p" ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in compat-host Step2: $f"
    exit 1
  fi
done

echo "[review] marker checks"
test -f scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py
test -f scripts/ci/exp-mcp-fragmented-ipi/compat_host/README.md
rg -n '"name": "read_document"' scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py >/dev/null || {
  echo "FAIL: compat host missing read_document tool"
  exit 1
}
rg -n '"name": "web_search"' scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py >/dev/null || {
  echo "FAIL: compat host missing web_search tool"
  exit 1
}
rg -n 'COMPAT_ROOT is required' scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py >/dev/null || {
  echo "FAIL: compat host missing COMPAT_ROOT preflight"
  exit 1
}

echo "[review] run compat-host smoke"
bash scripts/ci/test-exp-mcp-fragmented-ipi-compat-host.sh

echo "[review] done"
