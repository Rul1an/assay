#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EVENT_NAME="${EVENT_NAME:-}"
RELEASE_VERSION_INPUT="${RELEASE_VERSION_INPUT:-}"
RELEASE_REF="${RELEASE_REF:-${GITHUB_REF:-}}"

case "$EVENT_NAME" in
  workflow_dispatch)
    requested="$RELEASE_VERSION_INPUT"
    ;;
  push)
    if [[ "$RELEASE_REF" != refs/tags/* ]]; then
      echo "release push must target a tag ref: ${RELEASE_REF:-<empty>}" >&2
      exit 1
    fi
    requested="${RELEASE_REF#refs/tags/}"
    ;;
  *)
    echo "unsupported release event: ${EVENT_NAME:-<empty>}" >&2
    exit 1
    ;;
esac

if [[ "$requested" == *$'\n'* || "$requested" == *$'\r'* ]]; then
  echo "release version must be a single-line value" >&2
  exit 1
fi
if [[ ! "$requested" =~ ^v[0-9]+[.][0-9]+[.][0-9]+(-(rc|beta)[.][0-9]+)?$ ]]; then
  echo "release version must be vX.Y.Z, vX.Y.Z-rc.N, or vX.Y.Z-beta.N: ${requested:-<empty>}" >&2
  exit 1
fi

workspace_version="$(
  awk '
    $0 == "[workspace.package]" { in_workspace_package = 1; next }
    /^\[/ && $0 != "[workspace.package]" { in_workspace_package = 0 }
    in_workspace_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' "$REPO_ROOT/Cargo.toml"
)"
if [[ ! "$workspace_version" =~ ^[0-9]+[.][0-9]+[.][0-9]+(-(rc|beta)[.][0-9]+)?$ ]]; then
  echo "could not read a supported workspace version from Cargo.toml" >&2
  exit 1
fi

expected="v${workspace_version}"
if [[ "$requested" != "$expected" ]]; then
  echo "release version ${requested} does not match workspace version ${expected}" >&2
  exit 1
fi

printf '%s\n' "$requested"
