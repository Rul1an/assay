#!/usr/bin/env bash
# Bind a release commit to main before anything is built or published.
#
# The required checks (CI, review-record-check, lane-check/proof, host-capability-check) protect
# `main`. A tag push or a dispatch on any other commit used to publish to every registry without
# meeting any of them. This check refuses a release commit that is not an ancestor of main, unless
# the release tag is an annotated tag on that same commit with exactly one line
#
#   Release-Source-Exception: <reason of at least 20 characters>
#
# An accepted exception is a declaration, not a proof: it is announced as a warning and written to
# the job summary so the off-main release is visible rather than silent.
#
# Environment:
#   RELEASE_SHA       commit being released (40 lowercase hex), normally ${{ github.sha }}
#   RELEASE_TAG       release tag, vX.Y.Z / vX.Y.Z-rc.N / vX.Y.Z-beta.N
#   RELEASE_MAIN_REF  ref that holds main's history (default refs/remotes/origin/main)
#   RELEASE_REMOTE    remote the annotated tag is read from (default origin)
#
# Exit status: 0 accepted, 1 refused, 2 usage error. Anything the check cannot read (a shallow
# checkout, a missing main ref, an unreachable remote) is a refusal, never a pass.
set -euo pipefail

RELEASE_SHA="${RELEASE_SHA:-}"
RELEASE_TAG="${RELEASE_TAG:-}"
RELEASE_MAIN_REF="${RELEASE_MAIN_REF:-refs/remotes/origin/main}"
RELEASE_REMOTE="${RELEASE_REMOTE:-origin}"
REASON_FLOOR=20
EXCEPTION_KEY="Release-Source-Exception:"
TAG_REF="refs/assay-release-source/tag"

refuse() { printf 'release source binding: %s\n' "$*" >&2; exit 1; }

[[ "$RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]] ||
  refuse "RELEASE_SHA must be 40 lowercase hex characters, got '${RELEASE_SHA}'"
[[ "$RELEASE_TAG" =~ ^v[0-9]+[.][0-9]+[.][0-9]+(-(rc|beta)[.][0-9]+)?$ ]] ||
  refuse "release tag must be vX.Y.Z, vX.Y.Z-rc.N or vX.Y.Z-beta.N, got '${RELEASE_TAG}'"

if [[ "$(git rev-parse --is-shallow-repository)" == "true" ]]; then
  refuse "the checkout is shallow, so it cannot show whether ${RELEASE_SHA} is on main; check out with fetch-depth: 0"
fi
git cat-file -e "${RELEASE_SHA}^{commit}" 2>/dev/null ||
  refuse "commit ${RELEASE_SHA} is not present in this checkout"
main_sha="$(git rev-parse --verify --quiet "${RELEASE_MAIN_REF}^{commit}")" ||
  refuse "cannot read ${RELEASE_MAIN_REF}; it must hold main's history"

if git merge-base --is-ancestor "$RELEASE_SHA" "$main_sha"; then
  printf 'release source %s is on main (%s at %s)\n' "$RELEASE_SHA" "$RELEASE_MAIN_REF" "$main_sha"
  exit 0
fi

# Off main: accept only a declared exception on an annotated tag that names this exact commit.
# The tag is read from the remote into a private ref, because a checkout can replace an annotated
# tag with a lightweight one and drop its message.
exception_reason() {
  git update-ref -d "$TAG_REF" 2>/dev/null || true
  git fetch --quiet --no-tags "$RELEASE_REMOTE" "+refs/tags/${RELEASE_TAG}:${TAG_REF}" 2>/dev/null ||
    return 1
  [[ "$(git cat-file -t "$TAG_REF")" == "tag" ]] || return 1
  [[ "$(git rev-parse "${TAG_REF}^{commit}")" == "$RELEASE_SHA" ]] || return 1
  local lines
  lines="$(git cat-file tag "$TAG_REF" | sed '1,/^$/d' | grep -E "^${EXCEPTION_KEY} " || true)"
  [[ -n "$lines" && "$(printf '%s\n' "$lines" | wc -l | tr -d ' ')" == "1" ]] || return 1
  local reason="${lines#"${EXCEPTION_KEY} "}"
  reason="$(printf '%s' "$reason" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//')"
  [[ "${#reason}" -ge "$REASON_FLOOR" ]] || return 1
  printf '%s' "$reason"
}

if reason="$(exception_reason)"; then
  git update-ref -d "$TAG_REF" 2>/dev/null || true
  message="release source ${RELEASE_SHA} is not on main; ${RELEASE_TAG} declares a Release-Source-Exception: ${reason}"
  printf '::warning title=Off-main release::%s\n' "$message"
  if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
    printf '### Off-main release\n\n%s\n' "$message" >> "$GITHUB_STEP_SUMMARY"
  fi
  exit 0
fi
git update-ref -d "$TAG_REF" 2>/dev/null || true

refuse "release source ${RELEASE_SHA} is not on main (${RELEASE_MAIN_REF} at ${main_sha}), and ${RELEASE_TAG} is not an annotated tag on that commit with exactly one '${EXCEPTION_KEY} <reason>' line of at least ${REASON_FLOOR} characters"
