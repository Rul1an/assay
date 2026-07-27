#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXPECTED_ERROR="latest Assay release is not a stable software tag"
FAKE_TAG="privileged-mcp-action-v0-candidate.2"
WORKSPACE_VERSION="$(
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
if [[ ! "$WORKSPACE_VERSION" =~ ^[0-9]+[.][0-9]+[.][0-9]+$ ]]; then
  echo "could not read a stable workspace version from Cargo.toml" >&2
  exit 1
fi
VALID_TAG="v${WORKSPACE_VERSION}"
LATEST_TAG="v1.0.0"
export FAKE_ASSAY_VERSION="$WORKSPACE_VERSION"
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
require_literal ".github/workflows/release.yml" \
  'version="$(bash scripts/ci/resolve-release-version.sh)"'
require_literal ".github/workflows/release.yml" \
  'VERSION: ${{ needs.release-contract.outputs.version }}'
MCP_REGISTRY_WORKFLOW=".github/workflows/mcp-registry-publish.yml"
require_literal "$MCP_REGISTRY_WORKFLOW" \
  "if: \${{ github.event_name != 'release' || github.event.release.prerelease == false }}"
require_literal "$MCP_REGISTRY_WORKFLOW" \
  'tag="$(bash scripts/ci/resolve-mcp-registry-tag.sh)"'
require_literal "$MCP_REGISTRY_WORKFLOW" 'MCP_PUBLISHER_VERSION: "v1.7.9"'
require_literal "$MCP_REGISTRY_WORKFLOW" \
  'MCP_PUBLISHER_LINUX_AMD64_SHA256: "ab128162b0616090b47cf245afe0a23f3ef08936fdce19074f5ba0a4469281ac"'
require_literal "$MCP_REGISTRY_WORKFLOW" \
  'printf '\''%s  %s\n'\'' "$MCP_PUBLISHER_LINUX_AMD64_SHA256" "$archive" | sha256sum -c -'
if grep -Fq -- "/releases/latest/" "$REPO_ROOT/$MCP_REGISTRY_WORKFLOW"; then
  echo "$MCP_REGISTRY_WORKFLOW must not execute an unpinned latest publisher" >&2
  exit 1
fi

for consumer in \
  scripts/install.sh \
  assay-action/resolve-version.sh \
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
printf '{"tag_name":"%s"}\n' "\${FAKE_TAG:?}"
EOF
cat >"$TMP_DIR/bin/jq" <<EOF
#!/usr/bin/env bash
cat >/dev/null
printf '%s\n' "\${FAKE_TAG:?}"
EOF
cat >"$TMP_DIR/bin/assay" <<'EOF'
#!/usr/bin/env bash
if [[ "${FAKE_MUTATE_ACTION_STATE:-0}" == "1" ]]; then
  [[ -z "${GITHUB_OUTPUT:-}" ]] ||
    printf 'resolved_version=v1\nresolved_version_plain=1.1.0\n' >>"$GITHUB_OUTPUT"
  [[ -z "${GITHUB_PATH:-}" ]] || printf '/tmp/untrusted\n' >>"$GITHUB_PATH"
  [[ -z "${GITHUB_ENV:-}" ]] || printf 'ASSAY_UNTRUSTED=1\n' >>"$GITHUB_ENV"
  [[ -z "${GITHUB_STATE:-}" ]] || printf 'assay_untrusted=1\n' >>"$GITHUB_STATE"
  [[ -z "${GITHUB_STEP_SUMMARY:-}" ]] || printf 'untrusted summary\n' >>"$GITHUB_STEP_SUMMARY"
fi
[[ -z "${FAKE_INVOCATION_COUNTER:-}" ]] || printf '1\n' >>"$FAKE_INVOCATION_COUNTER"
printf 'assay %s\n' "${FAKE_ASSAY_VERSION:-3.35.0}"
EOF
cat >"$TMP_DIR/bin/multipass" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "${FAKE_VM_VERSION:?}"
EOF
chmod +x \
  "$TMP_DIR/bin/curl" \
  "$TMP_DIR/bin/jq" \
  "$TMP_DIR/bin/assay" \
  "$TMP_DIR/bin/multipass"

rejects_fake_latest() {
  local output
  if output=$(FAKE_TAG="$FAKE_TAG" PATH="$TMP_DIR/bin:$PATH" "$@" 2>&1); then
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
rejects_fake_latest env GITHUB_OUTPUT="$TMP_DIR/action-invalid.out" \
  bash "$REPO_ROOT/assay-action/resolve-version.sh" latest
# shellcheck disable=SC2016 # $1 expands in the nested shell.
rejects_fake_latest bash -c \
  'source "$1"; latest_assay_tag' _ "$REPO_ROOT/infra/bpf-runner/health_check.sh"

FAKE_TAG="$VALID_TAG" GITHUB_OUTPUT="$TMP_DIR/action-valid.out" \
  PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/assay-action/resolve-version.sh" latest
require_literal_from_path() {
  local file="$1"
  local literal="$2"
  if ! grep -Fq -- "$literal" "$file"; then
    echo "$file must contain: $literal" >&2
    exit 1
  fi
}

run_logged() {
  local log_file="$1"
  shift
  if ! "$@" >"$log_file" 2>&1; then
    cat "$log_file" >&2
    return 1
  fi
}

resolved_dispatch="$(
  EVENT_NAME=workflow_dispatch RELEASE_VERSION_INPUT="$VALID_TAG" \
    bash "$REPO_ROOT/scripts/ci/resolve-release-version.sh"
)"
if [[ "$resolved_dispatch" != "$VALID_TAG" ]]; then
  echo "release resolver changed a workspace-matching dispatch version" >&2
  exit 1
fi

resolved_push="$(
  EVENT_NAME=push RELEASE_REF="refs/tags/$VALID_TAG" \
    bash "$REPO_ROOT/scripts/ci/resolve-release-version.sh"
)"
if [[ "$resolved_push" != "$VALID_TAG" ]]; then
  echo "release resolver changed a workspace-matching pushed tag" >&2
  exit 1
fi

if EVENT_NAME=workflow_dispatch RELEASE_VERSION_INPUT="$LATEST_TAG" \
  bash "$REPO_ROOT/scripts/ci/resolve-release-version.sh" \
  >"$TMP_DIR/release-version-mismatch.log" 2>&1
then
  echo "release resolver accepted a tag that differs from the workspace version" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/release-version-mismatch.log" \
  "release version $LATEST_TAG does not match workspace version $VALID_TAG"

if EVENT_NAME=workflow_dispatch RELEASE_VERSION_INPUT="${VALID_TAG}"$'\nextra' \
  bash "$REPO_ROOT/scripts/ci/resolve-release-version.sh" \
  >"$TMP_DIR/release-version-injected.log" 2>&1
then
  echo "release resolver accepted a version containing a line break" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/release-version-injected.log" \
  "release version must be a single-line value"

if EVENT_NAME=workflow_dispatch RELEASE_VERSION_INPUT="${VALID_TAG}-rc.1" \
  bash "$REPO_ROOT/scripts/ci/resolve-release-version.sh" \
  >"$TMP_DIR/release-version-prerelease.log" 2>&1
then
  echo "release resolver accepted a prerelease tag for a stable workspace" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/release-version-prerelease.log" \
  "release version ${VALID_TAG}-rc.1 does not match workspace version $VALID_TAG"

mkdir -p "$TMP_DIR/prerelease-repo/scripts/ci"
cp "$REPO_ROOT/scripts/ci/resolve-release-version.sh" \
  "$TMP_DIR/prerelease-repo/scripts/ci/resolve-release-version.sh"
cat >"$TMP_DIR/prerelease-repo/Cargo.toml" <<'EOF'
[workspace]

[workspace.package]
version = "3.36.0-rc.1"
EOF
resolved_prerelease="$(
  EVENT_NAME=workflow_dispatch RELEASE_VERSION_INPUT="v3.36.0-rc.1" \
    bash "$TMP_DIR/prerelease-repo/scripts/ci/resolve-release-version.sh"
)"
if [[ "$resolved_prerelease" != "v3.36.0-rc.1" ]]; then
  echo "release resolver changed a workspace-matching prerelease version" >&2
  exit 1
fi

registry_release_tag="$(
  EVENT_NAME=release RELEASE_TAG="$VALID_TAG" RELEASE_PRERELEASE=false \
    bash "$REPO_ROOT/scripts/ci/resolve-mcp-registry-tag.sh"
)"
if [[ "$registry_release_tag" != "$VALID_TAG" ]]; then
  echo "MCP Registry resolver changed a stable release tag" >&2
  exit 1
fi

registry_dispatch_tag="$(
  EVENT_NAME=workflow_dispatch INPUT_VERSION="$VALID_TAG" \
    bash "$REPO_ROOT/scripts/ci/resolve-mcp-registry-tag.sh"
)"
if [[ "$registry_dispatch_tag" != "$VALID_TAG" ]]; then
  echo "MCP Registry resolver changed a stable dispatch tag" >&2
  exit 1
fi

if EVENT_NAME=release RELEASE_TAG="v3.36.0-rc.1" RELEASE_PRERELEASE=true \
  bash "$REPO_ROOT/scripts/ci/resolve-mcp-registry-tag.sh" \
  >"$TMP_DIR/registry-prerelease-event.log" 2>&1
then
  echo "MCP Registry resolver accepted a GitHub prerelease" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/registry-prerelease-event.log" \
  "MCP Registry publication is limited to stable GitHub releases"

if EVENT_NAME=workflow_dispatch INPUT_VERSION="v3.36.0-beta.1" \
  bash "$REPO_ROOT/scripts/ci/resolve-mcp-registry-tag.sh" \
  >"$TMP_DIR/registry-prerelease-dispatch.log" 2>&1
then
  echo "MCP Registry resolver accepted a prerelease dispatch tag" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/registry-prerelease-dispatch.log" \
  "MCP Registry publication requires a stable vX.Y.Z release tag"

if EVENT_NAME=workflow_dispatch INPUT_VERSION="${VALID_TAG}"$'\nextra' \
  bash "$REPO_ROOT/scripts/ci/resolve-mcp-registry-tag.sh" \
  >"$TMP_DIR/registry-injected-dispatch.log" 2>&1
then
  echo "MCP Registry resolver accepted a tag containing a line break" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/registry-injected-dispatch.log" \
  "MCP Registry release tag must be a single-line value"

require_literal_from_path "$TMP_DIR/action-valid.out" "resolved_version=$VALID_TAG"
require_literal_from_path "$TMP_DIR/action-valid.out" "resolved_version_plain=${VALID_TAG#v}"
require_literal_from_path "$TMP_DIR/action-valid.out" "skip_install=true"

for state_file in output path env state summary; do
  : >"$TMP_DIR/action-child-$state_file.out"
done
: >"$TMP_DIR/action-child-invocations.out"
FAKE_MUTATE_ACTION_STATE=1 FAKE_ASSAY_VERSION="$WORKSPACE_VERSION" \
  FAKE_INVOCATION_COUNTER="$TMP_DIR/action-child-invocations.out" \
  GITHUB_OUTPUT="$TMP_DIR/action-child-output.out" \
  GITHUB_PATH="$TMP_DIR/action-child-path.out" \
  GITHUB_ENV="$TMP_DIR/action-child-env.out" \
  GITHUB_STATE="$TMP_DIR/action-child-state.out" \
  GITHUB_STEP_SUMMARY="$TMP_DIR/action-child-summary.out" \
  PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/assay-action/resolve-version.sh" "$VALID_TAG"
require_literal_from_path "$TMP_DIR/action-child-output.out" \
  "resolved_version=$VALID_TAG"
require_literal_from_path "$TMP_DIR/action-child-output.out" \
  "resolved_version_plain=${VALID_TAG#v}"
require_literal_from_path "$TMP_DIR/action-child-output.out" "skip_install=true"
if grep -Fq -- "resolved_version=v1" "$TMP_DIR/action-child-output.out"; then
  echo "pre-existing assay binary mutated GITHUB_OUTPUT directly" >&2
  exit 1
fi
for state_file in path env state summary; do
  if [[ -s "$TMP_DIR/action-child-$state_file.out" ]]; then
    echo "pre-existing assay binary mutated GitHub Action state: $state_file" >&2
    exit 1
  fi
done
if [[ "$(wc -l <"$TMP_DIR/action-child-invocations.out" | tr -d ' ')" != "1" ]]; then
  echo "assay resolver invoked the inspected binary more than once" >&2
  exit 1
fi

PRERELEASE_TAG="3.36.0-rc.1"
FAKE_TAG="$VALID_TAG" GITHUB_OUTPUT="$TMP_DIR/action-prerelease.out" \
  PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/assay-action/resolve-version.sh" "$PRERELEASE_TAG"
require_literal_from_path "$TMP_DIR/action-prerelease.out" \
  "resolved_version=v$PRERELEASE_TAG"

COMPAT_TAG="v2.1"
FAKE_TAG="$VALID_TAG" FAKE_ASSAY_VERSION="2.1.0" \
  GITHUB_OUTPUT="$TMP_DIR/action-compat.out" \
  PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/assay-action/resolve-version.sh" "$COMPAT_TAG"
require_literal_from_path "$TMP_DIR/action-compat.out" \
  "resolved_version=$COMPAT_TAG"
require_literal_from_path "$TMP_DIR/action-compat.out" \
  "resolved_version_plain=2.1.0"
require_literal_from_path "$TMP_DIR/action-compat.out" "skip_install=true"

for COMPAT_CASE in "v1 1.1.0" "v2 2.12.0"; do
  read -r COMPAT_ALIAS COMPAT_VERSION <<<"$COMPAT_CASE"
  FAKE_ASSAY_VERSION="$COMPAT_VERSION" \
    GITHUB_OUTPUT="$TMP_DIR/action-${COMPAT_ALIAS}.out" \
    PATH="$TMP_DIR/bin:$PATH" \
    bash "$REPO_ROOT/assay-action/resolve-version.sh" "$COMPAT_ALIAS"
  require_literal_from_path "$TMP_DIR/action-${COMPAT_ALIAS}.out" \
    "resolved_version=$COMPAT_ALIAS"
  require_literal_from_path "$TMP_DIR/action-${COMPAT_ALIAS}.out" \
    "resolved_version_plain=$COMPAT_VERSION"
  require_literal_from_path "$TMP_DIR/action-${COMPAT_ALIAS}.out" "skip_install=true"

  : >"$TMP_DIR/action-${COMPAT_ALIAS}-verify.out"
  : >"$TMP_DIR/action-${COMPAT_ALIAS}-path.out"
  FAKE_ASSAY_VERSION="$COMPAT_VERSION" \
    GITHUB_OUTPUT="$TMP_DIR/action-${COMPAT_ALIAS}-verify.out" \
    GITHUB_PATH="$TMP_DIR/action-${COMPAT_ALIAS}-path.out" \
    bash "$REPO_ROOT/assay-action/verify-install.sh" \
    "$TMP_DIR/bin/assay" "$COMPAT_VERSION"
  require_literal_from_path "$TMP_DIR/action-${COMPAT_ALIAS}-verify.out" "installed=true"
done

: >"$TMP_DIR/action-verify.out"
: >"$TMP_DIR/action-path.out"
: >"$TMP_DIR/action-verify-invocations.out"
FAKE_ASSAY_VERSION="2.1.0" \
  FAKE_INVOCATION_COUNTER="$TMP_DIR/action-verify-invocations.out" \
  GITHUB_OUTPUT="$TMP_DIR/action-verify.out" \
  GITHUB_PATH="$TMP_DIR/action-path.out" \
  bash "$REPO_ROOT/assay-action/verify-install.sh" "$TMP_DIR/bin/assay" "2.1.0"
require_literal_from_path "$TMP_DIR/action-verify.out" "installed=true"
require_literal_from_path "$TMP_DIR/action-path.out" "$TMP_DIR/bin"
if [[ "$(wc -l <"$TMP_DIR/action-verify-invocations.out" | tr -d ' ')" != "1" ]]; then
  echo "successful assay verification invoked the inspected binary more than once" >&2
  exit 1
fi

for state_file in output path env state summary; do
  : >"$TMP_DIR/action-verify-mismatch-$state_file.out"
done
: >"$TMP_DIR/action-verify-mismatch-invocations.out"
if FAKE_MUTATE_ACTION_STATE=1 FAKE_ASSAY_VERSION="2.1.0" \
  FAKE_INVOCATION_COUNTER="$TMP_DIR/action-verify-mismatch-invocations.out" \
  GITHUB_OUTPUT="$TMP_DIR/action-verify-mismatch-output.out" \
  GITHUB_PATH="$TMP_DIR/action-verify-mismatch-path.out" \
  GITHUB_ENV="$TMP_DIR/action-verify-mismatch-env.out" \
  GITHUB_STATE="$TMP_DIR/action-verify-mismatch-state.out" \
  GITHUB_STEP_SUMMARY="$TMP_DIR/action-verify-mismatch-summary.out" \
  bash "$REPO_ROOT/assay-action/verify-install.sh" "$TMP_DIR/bin/assay" "2.1" \
  >"$TMP_DIR/action-verify-mismatch.log" 2>&1
then
  echo "assay-action accepted a binary version that did not match the expected version" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/action-verify-mismatch.log" \
  "expected 2.1, got 2.1.0"

MALFORMED_INSTALLED_VERSION=$'3.35.0\nx resolved_version=v1\nx resolved_version_plain=1.1.0'
: >"$TMP_DIR/action-malformed-installed.out"
FAKE_ASSAY_VERSION="$MALFORMED_INSTALLED_VERSION" \
  GITHUB_OUTPUT="$TMP_DIR/action-malformed-installed.out" \
  PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/assay-action/resolve-version.sh" "$VALID_TAG" \
  >"$TMP_DIR/action-malformed-installed.log" 2>&1
require_literal_from_path "$TMP_DIR/action-malformed-installed.out" \
  "resolved_version=$VALID_TAG"
require_literal_from_path "$TMP_DIR/action-malformed-installed.out" \
  "resolved_version_plain=${VALID_TAG#v}"
require_literal_from_path "$TMP_DIR/action-malformed-installed.out" "skip_install=false"
if grep -Fq -- "resolved_version=v1" "$TMP_DIR/action-malformed-installed.out"; then
  echo "assay-action let malformed installed-version output overwrite validated outputs" >&2
  exit 1
fi
if grep -Fq -- "resolved_version_plain=1.1.0" "$TMP_DIR/action-malformed-installed.out"; then
  echo "assay-action retained injected output from a malformed installed binary" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/action-malformed-installed.log" \
  "Assay already installed: unknown"
for state_file in output path env state summary; do
  if [[ -s "$TMP_DIR/action-verify-mismatch-$state_file.out" ]]; then
    echo "rejected assay binary mutated GitHub Action state: $state_file" >&2
    exit 1
  fi
done
if [[ "$(wc -l <"$TMP_DIR/action-verify-mismatch-invocations.out" | tr -d ' ')" != "1" ]]; then
  echo "assay verifier invoked the inspected binary more than once" >&2
  exit 1
fi

for INJECTED_VERSION in $'3.35.0\nskip_install=true' $'3.35.0\rskip_install=true'; do
  : >"$TMP_DIR/action-injected.out"
  if GITHUB_OUTPUT="$TMP_DIR/action-injected.out" PATH="$TMP_DIR/bin:$PATH" \
    bash "$REPO_ROOT/assay-action/resolve-version.sh" "$INJECTED_VERSION" \
    >"$TMP_DIR/action-injected.log" 2>&1
  then
    echo "assay-action accepted a version input containing a line break" >&2
    exit 1
  fi
  if [[ -s "$TMP_DIR/action-injected.out" ]]; then
    echo "assay-action wrote outputs before rejecting a version input containing a line break" >&2
    exit 1
  fi
  require_literal_from_path "$TMP_DIR/action-injected.log" \
    "Assay version must not contain line breaks"
done

health_tag=$(
  # shellcheck disable=SC2016 # $1 expands in the nested shell.
  FAKE_TAG="$VALID_TAG" PATH="$TMP_DIR/bin:$PATH" \
    bash -c 'source "$1"; latest_assay_tag' _ \
    "$REPO_ROOT/infra/bpf-runner/health_check.sh"
)
if [[ "$health_tag" != "$VALID_TAG" ]]; then
  echo "health_check.sh rejected stable latest tag $VALID_TAG" >&2
  exit 1
fi

mkdir -p "$TMP_DIR/harness/.github/workflows"
cat >"$TMP_DIR/harness/.github/workflows/harness-ci.yml" <<'EOF'
on:
  workflow_dispatch:
    inputs:
      unrelated_version:
        default: "v9.9.9"
      assay_version:
        default: "v3.27.0"
EOF
run_logged "$TMP_DIR/version-line-release-prep.log" \
  env FAKE_TAG="$LATEST_TAG" EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=0 \
  HARNESS_DIR="$TMP_DIR/harness" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh"
require_literal_from_path "$TMP_DIR/version-line-release-prep.log" \
  "latest_release=$LATEST_TAG"
require_literal_from_path "$TMP_DIR/version-line-release-prep.log" \
  "release_target=$VALID_TAG"
require_literal_from_path "$TMP_DIR/version-line-release-prep.log" \
  "harness_compatibility_assay_version=v3.27.0"
require_literal_from_path "$TMP_DIR/version-line-release-prep.log" \
  "version_line_status=ok"

run_logged "$TMP_DIR/version-line-vm-latest.log" \
  env FAKE_TAG="$LATEST_TAG" FAKE_VM_VERSION="${LATEST_TAG#v}" \
  EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=1 \
  HARNESS_DIR="$TMP_DIR/harness" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh"
require_literal_from_path "$TMP_DIR/version-line-vm-latest.log" \
  "vm_assay_version=${LATEST_TAG#v}"
require_literal_from_path "$TMP_DIR/version-line-vm-latest.log" \
  "version_line_status=ok"

if FAKE_TAG="$LATEST_TAG" FAKE_VM_VERSION="$WORKSPACE_VERSION" \
  EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=1 \
  HARNESS_DIR="$TMP_DIR/harness" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh" \
  >"$TMP_DIR/version-line-vm-target.log" 2>&1
then
  echo "version-line check accepted a VM on the release target instead of Latest" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/version-line-vm-target.log" \
  "VM assay version $VALID_TAG does not match latest release $LATEST_TAG"

mkdir -p "$TMP_DIR/harness-lookalike/.github/workflows"
cat >"$TMP_DIR/harness-lookalike/.github/workflows/harness-ci.yml" <<'EOF'
on:
  push:
jobs:
  decoy:
    runs-on: ubuntu-latest
    steps:
      - run: |
          workflow_dispatch:
            inputs:
              assay_version:
                default: "v9.9.9"
EOF
if FAKE_TAG="$LATEST_TAG" EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=0 \
  HARNESS_DIR="$TMP_DIR/harness-lookalike" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh" \
  >"$TMP_DIR/version-line-lookalike.log" 2>&1
then
  echo "version-line check accepted workflow YAML from a block scalar" >&2
  exit 1
fi

mkdir -p "$TMP_DIR/harness-duplicate/.github/workflows"
cat >"$TMP_DIR/harness-duplicate/.github/workflows/harness-ci.yml" <<'EOF'
on:
  workflow_dispatch:
    inputs:
      assay_version:
        default: "v3.27.0"
        default: "v9.9.9"
EOF
if FAKE_TAG="$LATEST_TAG" EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=0 \
  HARNESS_DIR="$TMP_DIR/harness-duplicate" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh" \
  >"$TMP_DIR/version-line-duplicate.log" 2>&1
then
  echo "version-line check accepted a duplicate Harness version key" >&2
  exit 1
fi

mkdir -p "$TMP_DIR/harness-semantic-duplicate/.github/workflows"
cat >"$TMP_DIR/harness-semantic-duplicate/.github/workflows/harness-ci.yml" <<'EOF'
on:
  workflow_dispatch:
    inputs:
      assay_version:
        default: "not-a-tag"
true:
  workflow_dispatch:
    inputs:
      assay_version:
        default: "v9.9.9"
EOF
if FAKE_TAG="$LATEST_TAG" EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=0 \
  HARNESS_DIR="$TMP_DIR/harness-semantic-duplicate" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh" \
  >"$TMP_DIR/version-line-semantic-duplicate.log" 2>&1
then
  echo "version-line check accepted semantically duplicate YAML keys" >&2
  exit 1
fi

mkdir -p "$TMP_DIR/harness-tagged-duplicate/.github/workflows"
cat >"$TMP_DIR/harness-tagged-duplicate/.github/workflows/harness-ci.yml" <<'EOF'
!!bool "true":
  workflow_dispatch:
    inputs:
      assay_version:
        default: "not-a-tag"
true:
  workflow_dispatch:
    inputs:
      assay_version:
        default: "v9.9.9"
EOF
if FAKE_TAG="$LATEST_TAG" EXPECTED_RELEASE="$VALID_TAG" CHECK_VM=0 \
  HARNESS_DIR="$TMP_DIR/harness-tagged-duplicate" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh" \
  >"$TMP_DIR/version-line-tagged-duplicate.log" 2>&1
then
  echo "version-line check accepted an explicitly tagged YAML key" >&2
  exit 1
fi

INVALID_TARGET="$FAKE_TAG"
if FAKE_TAG="$LATEST_TAG" EXPECTED_RELEASE="$INVALID_TARGET" CHECK_VM=0 \
  HARNESS_DIR="$TMP_DIR/harness" PATH="$TMP_DIR/bin:$PATH" \
  bash "$REPO_ROOT/scripts/ci/check-assay-version-line.sh" \
  >"$TMP_DIR/version-line-invalid-target.log" 2>&1
then
  echo "version-line check accepted a non-software release target" >&2
  exit 1
fi
require_literal_from_path "$TMP_DIR/version-line-invalid-target.log" \
  "expected release is not a stable software tag: $INVALID_TARGET"

echo "release channel separation contract passed"
