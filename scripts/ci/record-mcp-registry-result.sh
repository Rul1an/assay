#!/usr/bin/env bash
set -euo pipefail

# Record the MCP Registry publication's terminal result on the GitHub release
# record, so the release itself links the registry run and its outcome.
#
# Inputs (env):
#   VERSION  release tag, e.g. v3.35.0
#   RESULT   terminal job result: success | failure | cancelled | skipped
#            (skipped = the prerelease exclusion; recorded explicitly so a
#            marker-less release stays distinguishable from one that predates
#            the release transaction — absence must never read clean)
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
  success|failure|cancelled)
    label="$RESULT"
    ;;
  skipped)
    # Observe the reason instead of asserting it: this script cannot see WHY
    # the publish job skipped, but the version itself carries the one
    # condition the publish gate skips on today. Any other skip cause gets a
    # bare "skipped" rather than a claimed explanation.
    if [[ "$VERSION" == *-rc* || "$VERSION" == *-beta* ]]; then
      label="skipped (prerelease; stable releases only)"
    else
      label="skipped"
    fi
    ;;
  *)
    echo "RESULT must be a terminal job result (success|failure|cancelled|skipped), got: ${RESULT:-<empty>}" >&2
    exit 1
    ;;
esac

notes="$(mktemp)"
trap 'rm -f "$notes" "${notes}.next"' EXIT

gh release view "$VERSION" --json body --jq .body > "$notes"

grep -vF "$MARKER" "$notes" > "${notes}.next" || true
mv "${notes}.next" "$notes"

printf '\n%s MCP Registry publication: %s (%s)\n' "$MARKER" "$label" "$RUN_URL" >> "$notes"

gh release edit "$VERSION" --notes-file "$notes"
