#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXPECTED_ERROR="latest Assay release is not a stable software tag"
FAKE_TAG="privileged-mcp-action-v0-candidate.2"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

require_literal() {
  local file="$1"
  local literal="$2"
  if ! grep -Fq -- "$literal" "$REPO_ROOT/$file"; then
    echo "$file must contain: $literal" >&2
    exit 1
  fi
}

PACK_WORKFLOW=".github/workflows/privileged-mcp-action-pack-release.yml"
require_literal "$PACK_WORKFLOW" "--prerelease"
require_literal "$PACK_WORKFLOW" "--latest=false"

for consumer in \
  scripts/install.sh \
  assay-action/action.yml \
  infra/bpf-runner/update_assay_latest.sh \
  infra/bpf-runner/health_check.sh \
  scripts/ci/check-assay-version-line.sh
do
  require_literal "$consumer" "$EXPECTED_ERROR"
  require_literal "$consumer" "^v[0-9]+[.][0-9]+[.][0-9]+$"
done

mkdir -p "$TMP_DIR/bin"
cat >"$TMP_DIR/bin/curl" <<EOF
#!/usr/bin/env bash
printf '%s\n' '{"tag_name":"$FAKE_TAG"}'
EOF
cat >"$TMP_DIR/bin/jq" <<EOF
#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' '$FAKE_TAG'
EOF
chmod +x "$TMP_DIR/bin/curl" "$TMP_DIR/bin/jq"

rejects_fake_latest() {
  local output
  if output=$(PATH="$TMP_DIR/bin:$PATH" "$@" 2>&1); then
    echo "$*: accepted non-software latest tag $FAKE_TAG" >&2
    exit 1
  fi
  if ! grep -Fq -- "$EXPECTED_ERROR: $FAKE_TAG" <<<"$output"; then
    echo "$*: did not report the rejected latest tag" >&2
    printf '%s\n' "$output" >&2
    exit 1
  fi
}

rejects_fake_latest bash "$REPO_ROOT/scripts/install.sh"
rejects_fake_latest bash "$REPO_ROOT/infra/bpf-runner/update_assay_latest.sh"
rejects_fake_latest env CHECK_VM=0 HARNESS_DIR="$TMP_DIR/missing-harness" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh"

echo "release channel separation contract passed"
