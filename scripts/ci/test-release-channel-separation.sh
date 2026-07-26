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
chmod +x "$TMP_DIR/bin/curl" "$TMP_DIR/bin/jq" "$TMP_DIR/bin/assay"

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

VALID_TAG="v3.35.0"
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
require_literal_from_path "$TMP_DIR/action-valid.out" "resolved_version=$VALID_TAG"
require_literal_from_path "$TMP_DIR/action-valid.out" "resolved_version_plain=${VALID_TAG#v}"
require_literal_from_path "$TMP_DIR/action-valid.out" "skip_install=true"

for state_file in output path env state summary; do
  : >"$TMP_DIR/action-child-$state_file.out"
done
: >"$TMP_DIR/action-child-invocations.out"
FAKE_MUTATE_ACTION_STATE=1 FAKE_ASSAY_VERSION="3.35.0" \
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
FAKE_ASSAY_VERSION="2.1.0" GITHUB_OUTPUT="$TMP_DIR/action-verify.out" \
  GITHUB_PATH="$TMP_DIR/action-path.out" \
  bash "$REPO_ROOT/assay-action/verify-install.sh" "$TMP_DIR/bin/assay" "2.1.0"
require_literal_from_path "$TMP_DIR/action-verify.out" "installed=true"
require_literal_from_path "$TMP_DIR/action-path.out" "$TMP_DIR/bin"

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

echo "release channel separation contract passed"
