#!/usr/bin/env bash
# Ordinary CI lightweight-only classifier. Empty input is not lightweight.
#
# A change is lightweight only when every path is one no Rust test reads: docs/archive/ (history,
# skipped by every docs walker), mkdocs.yml, scripts/ci/review-*.sh and the issue and discussion
# templates. Active docs/ and every Markdown file outside docs/archive/ take the full lane:
# docs_corpus_loads, config_reference_examples_parse and documented_trace_events_read walk them,
# and include_str! readers in integration tests and in assay-cli's binary target compare them with
# the code.
# crates/ and packaging/agent-plugin/ are never lightweight, Markdown included.
set -euo pipefail

list="${1:-/dev/stdin}"

if [[ ! -s "$list" ]]; then
  echo false
  exit 0
fi

if grep -E '^(crates|packaging/agent-plugin)/' "$list" >/dev/null; then
  echo false
  exit 0
fi

if grep -Ev '^(docs/archive/.+|mkdocs\.yml|scripts/ci/review-[^/]+\.sh|\.github/ISSUE_TEMPLATE/[^/]+\.ya?ml|\.github/DISCUSSION_TEMPLATE/[^/]+\.ya?ml)$' "$list" >/dev/null; then
  echo false
  exit 0
fi

echo true
