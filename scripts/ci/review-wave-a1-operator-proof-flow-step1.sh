#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

if ! git rev-parse --verify --quiet "${BASE_REF}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${BASE_REF}" >&2
  exit 1
fi

guide="docs/guides/operator-proof-flow.md"
docs_index="docs/index.md"
getting_started="docs/getting-started/index.md"
guides_index="docs/guides/index.md"
mcp_quickstart="docs/mcp/quickstart.md"
release_doc="docs/reference/release.md"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! rg -ni -- "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

check_lacks_match() {
  local pattern="$1"
  local file="$2"
  if rg -ni -- "$pattern" "$file" >/dev/null; then
    echo "unexpected pattern in ${file}: ${pattern}" >&2
    exit 1
  fi
}

echo "== syntax =="
bash -n "$0"

echo "== guide anchors =="
check_has_match '^# Operator Proof Flow$' "$guide"
check_has_match 'assay import --format streamable-http' "$guide"
check_has_match 'assay evidence lint .*owasp-agentic-control-evidence-baseline' "$guide"
check_has_match 'tests/fixtures/evidence/test-bundle\.tar\.gz' "$guide"
check_has_match 'A1-002' "$guide"
check_has_match 'A3-001' "$guide"
check_has_match 'A5-001' "$guide"
check_has_match 'verify-offline\.sh --assets-dir' "$guide"
check_has_match 'release_archive_inventory\.sh' "$guide"
check_has_match 'Canonical Summary' "$guide"
check_has_match 'control-evidence' "$guide"
check_has_match 'GitHub CLI with support for' "$guide"

echo "== navigation anchors =="
check_has_match 'Operator Proof Flow' "$docs_index"
check_has_match 'Operator Proof Flow' "$getting_started"
check_has_match 'Operator Proof Flow' "$guides_index"
check_has_match 'Operator Proof Flow' "$mcp_quickstart"
check_has_match 'Operator Flow Check' "$release_doc"

echo "== primary path anchors =="
check_has_match 'canonical consumer verification path' 'docs/security/RELEASE-PROOF-KIT.md'
check_has_match 'canonical path is offline verification' 'docs/security/RELEASE-PROOF-KIT.md'
check_has_match 'convenience-only' 'docs/security/RELEASE-PROOF-KIT.md'
check_has_match 'canonical verification path for this kit is `verify-offline\.sh`' "$guide"

echo "== banned overclaim phrases =="
for file in "$guide" "$docs_index" "$getting_started" "$guides_index" "$mcp_quickstart" "$release_doc"; do
  check_lacks_match 'detects goal hijack' "$file"
  check_lacks_match 'verifies privilege abuse' "$file"
  check_lacks_match 'proves sandboxing' "$file"
  check_lacks_match 'mandate linkage enforcement' "$file"
  check_lacks_match 'supply chain solved' "$file"
  check_lacks_match 'general Sigstore verification' "$file"
  check_lacks_match 'generic Rekor verification' "$file"
done

echo "== diff allowlist =="
leaks="$(rg -v \
  '^docs/guides/operator-proof-flow\.md$|^docs/index\.md$|^docs/getting-started/index\.md$|^docs/guides/index\.md$|^docs/mcp/quickstart\.md$|^docs/reference/release\.md$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|MOVE-MAP|REVIEW-PACK)-wave-a1-operator-proof-flow-step1\.md$|^scripts/ci/review-wave-a1-operator-proof-flow-step1\.sh$' \
  < <(git diff --name-only "${BASE_REF}...HEAD") || true)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:" >&2
  echo "$leaks" >&2
  exit 1
fi

echo "== whitespace =="
git diff --check "${BASE_REF}...HEAD"

echo "Wave A1 Step1 reviewer script: PASS"
