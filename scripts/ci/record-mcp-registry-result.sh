#!/usr/bin/env bash
set -euo pipefail

# Record the MCP Registry publication's terminal result on the GitHub release
# record, so the release itself links the registry run and its outcome.
#
# Inputs (env):
#   VERSION  release tag, e.g. v3.35.0
#   RESULT   terminal job result: success | failure | cancelled
#   RUN_URL  URL of the workflow run that carried the publication
#
# Idempotent: exactly one marker line survives, so retries and re-runs replace
# the previous status instead of stacking history in the release notes.

VERSION="${VERSION:-}"
RESULT="${RESULT:-}"
RUN_URL="${RUN_URL:-}"
MARKER="<!-- mcp-registry-status -->"

[[ -n "$VERSION" ]] || { echo "VERSION is required" >&2; exit 1; }
[[ -n "$RUN_URL" ]] || { echo "RUN_URL is required" >&2; exit 1; }

case "$RESULT" in
  success|failure|cancelled) ;;
  *)
    echo "RESULT must be a terminal job result (success|failure|cancelled), got: ${RESULT:-<empty>}" >&2
    exit 1
    ;;
esac

notes="$(mktemp)"
trap 'rm -f "$notes"' EXIT

gh release view "$VERSION" --json body --jq .body > "$notes"

grep -vF "$MARKER" "$notes" > "${notes}.next" || true
mv "${notes}.next" "$notes"

printf '\n%s MCP Registry publication: %s (%s)\n' "$MARKER" "$RESULT" "$RUN_URL" >> "$notes"

gh release edit "$VERSION" --notes-file "$notes"
