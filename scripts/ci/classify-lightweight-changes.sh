#!/usr/bin/env bash
# Ordinary CI lightweight-only classifier. Empty input is not lightweight.
# packaging/agent-plugin/** is never lightweight, including Markdown-only edits.
set -euo pipefail

list="${1:-/dev/stdin}"

if [[ ! -s "$list" ]]; then
  echo false
  exit 0
fi

if grep -E '^packaging/agent-plugin/' "$list" >/dev/null; then
  echo false
  exit 0
fi

if grep -Ev '^(docs/|.*\.md$|mkdocs\.yml$|scripts/ci/review-[^/]+\.sh$|\.github/ISSUE_TEMPLATE/[^/]+\.ya?ml$|\.github/DISCUSSION_TEMPLATE/[^/]+\.ya?ml$)' "$list" >/dev/null; then
  echo false
  exit 0
fi

echo true
