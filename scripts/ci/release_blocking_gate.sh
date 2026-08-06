#!/usr/bin/env bash
# Refuse to release over an open `release-blocking` issue in the release's milestone.
#
# Extracted from ~25 inline lines in `release.yml` (#1993). In one day that step acquired three
# fail-open defects, all the same shape -- the gate deciding it had nothing to check and exiting 0:
# a milestone lookup that read an API failure as "no such milestone", a lookup that read only the
# first page, and a version prefixed twice so it searched for `vv3.38.0`. Every one of them logged
# like success.
#
# It could not be exercised without cutting a release, so its only test was production and its
# failure mode was silence. That is the shape this repository refuses everywhere else. The two API
# calls go through `$GH_BIN` for the same reason `release_proof_kit_build.sh` does: the seam is what
# lets `test-release-blocking-gate.sh` drive the paths that have no other way to be reached.
#
# Usage: release_blocking_gate.sh <version> <owner/repo>
#
# Exit 0 = release may proceed. Exit 1 = refuse. There is no third outcome, and no override input:
# if a release must go out over a blocker, the way is to drop the label or move the issue to a later
# milestone, both of which are recorded on the issue itself.

set -euo pipefail

GH_BIN="${GH_BIN:-gh}"

version="${1:-}"
repo="${2:-}"

if [ -z "$version" ] || [ -z "$repo" ]; then
  echo "usage: $(basename "$0") <version> <owner/repo>" >&2
  exit 1
fi

# `::error::` only where something reads it. The decision above this line is the same either way,
# which is what makes the decision testable outside a job.
fail() {
  if [ "${GITHUB_ACTIONS:-}" = "true" ]; then
    echo "::error::$1"
  else
    echo "error: $1" >&2
  fi
}

# `resolve-release-version.sh` returns the tag form, WITH the leading v. Blindly prefixing another
# one produced `vv3.38.0`, which matches no milestone and takes the nothing-to-gate-on path -- a
# fail-open that logs like success, in the gate whose entire point is to fail closed. Normalise both
# known shapes and refuse anything else rather than guessing.
case "$version" in
  v[0-9]*) milestone="$version" ;;
  [0-9]*)  milestone="v$version" ;;
  *)
    fail "unrecognised release version '${version}'; refusing to release rather than guessing which milestone to check."
    exit 1
    ;;
esac

# Capture before testing, and paginate. Both matter, and both are fail-closed fixes:
#
#   - `if ! gh api ... | grep -q` would treat an API failure as "milestone not found", because
#     `set -e` is suspended inside an `if` condition and `!` inverts the pipeline's non-zero status.
#     A rate limit or a network blip would have released over an open blocker. Assigning first makes
#     a failure distinguishable from a miss.
#   - `--paginate` because this gate is meant to outlive the milestones it reads. A repo with more
#     than one page of them would find nothing on page 1 and disable the gate precisely when the
#     project is old enough to have forgotten it exists.
if ! titles="$("$GH_BIN" api --paginate "repos/${repo}/milestones?state=all" --jq '.[].title')"; then
  fail "could not list milestones; refusing to release rather than guessing."
  exit 1
fi

# A release with no matching milestone is not blocked. Milestones are optional here, and failing on
# their absence would push people to stop using them, which costs the gate its only input.
#
# This is the one deliberate open path in the gate. It is here, once, and
# `test-release-blocking-gate.sh` pins it so it stays deliberate rather than becoming the fourth
# accident.
if ! grep -Fxq "$milestone" <<<"$titles"; then
  echo "No milestone named ${milestone}; nothing to gate on."
  exit 0
fi

# Deliberately NOT inside an `if`. Under `set -e` a failed query aborts the script, which is the
# fail-closed behaviour a gate needs. Wrapping this in a condition would swallow the failure and
# release, which is the same shape as the milestone bug above.
blockers="$("$GH_BIN" issue list \
  --repo "$repo" \
  --label release-blocking \
  --milestone "$milestone" \
  --state open \
  --limit 100 \
  --json number,title \
  --jq '.[] | "  #\(.number) \(.title)"')"

if [ -n "$blockers" ]; then
  fail "${milestone} has open release-blocking issues; refusing to release."
  echo "$blockers"
  echo
  echo "Close them, drop the release-blocking label, or move them to a later milestone."
  exit 1
fi

echo "${milestone} has no open release-blocking issues."
