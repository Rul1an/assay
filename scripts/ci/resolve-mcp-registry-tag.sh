#!/usr/bin/env bash
set -euo pipefail

EVENT_NAME="${EVENT_NAME:-}"
RELEASE_TAG="${RELEASE_TAG:-}"
RELEASE_PRERELEASE="${RELEASE_PRERELEASE:-}"
INPUT_VERSION="${INPUT_VERSION:-}"

case "$EVENT_NAME" in
  release)
    if [[ "$RELEASE_PRERELEASE" != "false" ]]; then
      echo "MCP Registry publication is limited to stable GitHub releases" >&2
      exit 1
    fi
    tag="$RELEASE_TAG"
    ;;
  workflow_dispatch|push)
    # workflow_dispatch: direct recovery dispatch of this workflow.
    # push: this workflow ran as a workflow_call job inside release.yml on a tag
    # push; the caller passes the contract-validated version as the call input.
    # Both paths require an explicit version and fall through to the stable-tag
    # gate below, so a bare push with no input stays fail-closed.
    tag="$INPUT_VERSION"
    ;;
  *)
    echo "unsupported MCP Registry publish event: ${EVENT_NAME:-<empty>}" >&2
    exit 1
    ;;
esac

if [[ "$tag" == *$'\n'* || "$tag" == *$'\r'* ]]; then
  echo "MCP Registry release tag must be a single-line value" >&2
  exit 1
fi
if [[ ! "$tag" =~ ^v[0-9]+[.][0-9]+[.][0-9]+$ ]]; then
  echo "MCP Registry publication requires a stable vX.Y.Z release tag: ${tag:-<empty>}" >&2
  exit 1
fi

printf '%s\n' "$tag"
