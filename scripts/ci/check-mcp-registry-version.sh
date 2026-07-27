#!/usr/bin/env bash
set -euo pipefail

# Ask the MCP Registry whether the version we are about to publish already
# exists, and whether it matches the content we would publish.
#
# Inputs (env):
#   TAG                stable release tag, e.g. v3.35.0
#   LOCAL_SERVER_JSON  path to the server.json downloaded from the release
#   REGISTRY_BASE_URL  optional, defaults to the official registry
#
# Stdout: "published" when the exact version exists with identical content,
#         "absent" when the registry does not know the version yet.
# Exit non-zero on any ambiguity: version/tag disagreement, content mismatch,
# or an unexpected registry response. Callers use "published" to skip the
# publish step (idempotent retry) and run this check again after publishing as
# the terminal confirmation, so this script must never guess.

TAG="${TAG:-}"
LOCAL_SERVER_JSON="${LOCAL_SERVER_JSON:-}"
REGISTRY_BASE_URL="${REGISTRY_BASE_URL:-https://registry.modelcontextprotocol.io}"

[[ -n "$TAG" ]] || { echo "TAG is required" >&2; exit 1; }
[[ -s "$LOCAL_SERVER_JSON" ]] || { echo "LOCAL_SERVER_JSON must be a non-empty file" >&2; exit 1; }

name="$(jq -er '.name' "$LOCAL_SERVER_JSON")"
version="$(jq -er '.version' "$LOCAL_SERVER_JSON")"
local_sha="$(jq -er '.packages[0].fileSha256' "$LOCAL_SERVER_JSON")"
local_packages="$(jq -Sc '.packages' "$LOCAL_SERVER_JSON")"

if [[ "v${version}" != "$TAG" ]]; then
  echo "release tag ${TAG} does not match server.json version ${version}" >&2
  exit 1
fi

encoded_name="${name//\//%2F}"
url="${REGISTRY_BASE_URL}/v0/servers/${encoded_name}/versions/${version}"

body="$(mktemp)"
trap 'rm -f "$body"' EXIT

http_code="$(curl -sS --max-time 30 -o "$body" -w '%{http_code}' "$url")"

case "$http_code" in
  404)
    echo "absent"
    ;;
  200)
    remote_name="$(jq -er '.server.name' "$body")"
    remote_version="$(jq -er '.server.version' "$body")"
    remote_sha="$(jq -er '.server.packages[0].fileSha256' "$body")"
    remote_packages="$(jq -Sc '.server.packages' "$body")"
    if [[ "$remote_name" != "$name" || "$remote_version" != "$version" ]]; then
      echo "registry returned ${remote_name}@${remote_version}, expected ${name}@${version}" >&2
      exit 1
    fi
    if [[ "$remote_sha" != "$local_sha" ]]; then
      echo "registry version ${version} exists with fileSha256 ${remote_sha}," \
        "but this release would publish ${local_sha}; refusing to treat it as published" >&2
      exit 1
    fi
    # The whole package set must match, not just the first artifact digest:
    # a same-version entry with a different identifier, transport, or an
    # extra package is not "already published", it is a divergence.
    if [[ "$remote_packages" != "$local_packages" ]]; then
      echo "registry version ${version} exists with a different package set" >&2
      echo "  registry: ${remote_packages}" >&2
      echo "  release:  ${local_packages}" >&2
      exit 1
    fi
    echo "published"
    ;;
  *)
    echo "unexpected registry response ${http_code} from ${url}" >&2
    exit 1
    ;;
esac
