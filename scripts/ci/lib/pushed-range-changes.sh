# shellcheck shell=bash
# Sourced by the pre-push clippy hook and selectors that may skip a local battery
# (run-assay-action-pin-battery-if-affected.sh, run-editor-release-hook-precommit-consumer-if-affected.sh):
# one rule for when a pushed range can be trusted, kept in one place.
#
# Each function prints the reason and returns 1 when it cannot establish its part; the caller then
# runs its battery in full. Nothing here decides which paths are a battery's inputs.

# The push must name a range, and the checked-out HEAD, with no tracked change on top, must be the
# pushed commit: the battery reads HEAD's tree, so a different tree is not what was pushed.
pushed_range_ready() {
  local from="${PRE_COMMIT_FROM_REF:-}" to="${PRE_COMMIT_TO_REF:-}" head pushed
  if [[ -z "${from}" || -z "${to}" ]]; then
    echo "no pushed range (PRE_COMMIT_FROM_REF/PRE_COMMIT_TO_REF unset)"
    return 1
  fi
  head="$(git rev-parse --verify --quiet 'HEAD^{commit}')" || { echo "HEAD does not resolve"; return 1; }
  pushed="$(git rev-parse --verify --quiet "${to}^{commit}")" || {
    echo "pushed ref ${to} does not resolve"
    return 1
  }
  if [[ "${pushed}" != "${head}" ]]; then
    echo "pushed commit ${pushed} is not the checked-out HEAD ${head}"
    return 1
  fi
  if ! git diff --no-ext-diff --quiet HEAD -- || ! git diff --no-ext-diff --cached --quiet HEAD --; then
    echo "tracked files differ from HEAD"
    return 1
  fi
}

# Write the pushed paths to $1, NUL-separated, with both sides of a rename and every deleted path.
# FROM...TO diffs from the merge-base, so it is the old-to-new change only on a fast-forward: a
# force push or a reset to an older commit can drop changes that only the old tip had.
pushed_range_paths() {
  local out="$1" from="${PRE_COMMIT_FROM_REF:-}" to="${PRE_COMMIT_TO_REF:-}" ancestry=0
  if ! git diff --no-ext-diff --no-renames --name-only -z "${from}...${to}" -- >"${out}"; then
    echo "cannot list the pushed changes ${from}...${to}"
    return 1
  fi
  git merge-base --is-ancestor "${from}" "${to}" || ancestry=$?
  case "${ancestry}" in
    0) ;;
    1) echo "pushed range ${from}..${to} is not a fast-forward"; return 1 ;;
    *) echo "cannot decide whether ${from} is an ancestor of ${to} (git exit ${ancestry})"; return 1 ;;
  esac
}
