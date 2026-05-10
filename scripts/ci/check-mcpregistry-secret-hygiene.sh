#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

token_file_regex='(^|/)\.mcpregistry_'

fail_with_list() {
  local title="$1"
  local entries="$2"
  echo "FAIL: ${title}" >&2
  printf '%s\n' "$entries" | sed 's/^/  - /' >&2
  exit 1
}

tracked="$(git ls-files | grep -E "$token_file_regex" || true)"
if [[ -n "$tracked" ]]; then
  fail_with_list "MCP registry token files must never be tracked" "$tracked"
fi

commit_risk="$(git ls-files --others --exclude-standard | grep -E "$token_file_regex" || true)"
if [[ -n "$commit_risk" ]]; then
  fail_with_list "MCP registry token files are not ignored and could be committed" "$commit_risk"
fi

local_files="$(git ls-files --others -i --exclude-standard | grep -E "$token_file_regex" || true)"
if [[ -n "$local_files" ]]; then
  echo "WARN: local MCP registry token files exist in the repo." >&2
  printf '%s\n' "$local_files" | sed 's/^/  - /' >&2
  echo "WARN: keep them local only; rotate credentials if they may have leaked into logs, shell history, or artifacts." >&2
  if [[ "${ASSAY_FAIL_ON_LOCAL_MCPREGISTRY_TOKENS:-0}" == "1" ]]; then
    exit 1
  fi
fi

echo "MCP registry secret hygiene check passed"
