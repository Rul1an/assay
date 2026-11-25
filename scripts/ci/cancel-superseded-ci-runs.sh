#!/usr/bin/env bash
# Cancel the older CI runs of this pull request, without a concurrency group.
#
# Branch protection judges only the newest head, so a run on an older head holds runners for a
# result nobody reads. A workflow-level concurrency group cannot fix that here (#3653, reverted):
# a cancelled run still runs every job whose condition is always(), and a group admits one running
# run, so the newer run waited for that tail. This script runs as the first job of the newer run
# and cancels older runs directly; nothing waits on anything.
#
# Best effort by design: it never fails the run it belongs to for a reason outside its inputs. A
# fork pull request gets a read-only token, so its cancel calls are refused; that is reported and
# the run carries on. Bad inputs are a usage error (exit 2).
#
# Environment: REPO, PR_NUMBER, HEAD_SHA (40 hex), HEAD_REF, RUN_ID; WORKFLOW defaults to ci.yml.
# A run is cancelled only if it is older than this run (smaller id), on another head SHA, opened
# from this repository, attached to this pull request, and not finished.
set -euo pipefail

REPO="${REPO:-}"
PR_NUMBER="${PR_NUMBER:-}"
HEAD_SHA="${HEAD_SHA:-}"
HEAD_REF="${HEAD_REF:-}"
RUN_ID="${RUN_ID:-}"
WORKFLOW="${WORKFLOW:-ci.yml}"

usage() { printf 'cancel-superseded: %s\n' "$*" >&2; exit 2; }
[[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || usage "REPO must be owner/name"
[[ "$PR_NUMBER" =~ ^[1-9][0-9]*$ ]] || usage "PR_NUMBER must be a positive integer"
[[ "$RUN_ID" =~ ^[1-9][0-9]*$ ]] || usage "RUN_ID must be a positive integer"
[[ "$HEAD_SHA" =~ ^[0-9a-f]{40}$ ]] || usage "HEAD_SHA must be 40 lowercase hex characters"
[[ -n "$HEAD_REF" ]] || usage "HEAD_REF must be set"
[[ "$WORKFLOW" =~ ^[A-Za-z0-9_.-]+\.ya?ml$ ]] || usage "WORKFLOW must be a workflow file name"

# The branch goes in as a form field, never into the URL text: a branch name is chosen by whoever
# opens the pull request.
if ! runs_json="$(gh api -X GET "repos/${REPO}/actions/workflows/${WORKFLOW}/runs" \
    -f event=pull_request -f branch="$HEAD_REF" -f per_page=100)"; then
  echo "::notice title=Superseded runs::could not list ${WORKFLOW} runs; nothing cancelled"
  exit 0
fi

if ! ids="$(jq -r --arg repo "$REPO" --argjson pr "$PR_NUMBER" --arg sha "$HEAD_SHA" \
    --argjson run "$RUN_ID" '
  .workflow_runs[]
  | select(.id < $run)
  | select(.head_sha != $sha)
  | select(.head_repository.full_name == $repo)
  | select([.pull_requests[]?.number] | index($pr))
  | select(.status as $s | ["requested", "waiting", "pending", "queued", "in_progress"] | index($s))
  | .id' <<<"$runs_json")"; then
  echo "::notice title=Superseded runs::could not read the run list; nothing cancelled"
  exit 0
fi

if [[ -z "$ids" ]]; then
  echo "no older ${WORKFLOW} run of pull request #${PR_NUMBER} to cancel"
  exit 0
fi
while IFS= read -r id; do
  if gh api -X POST "repos/${REPO}/actions/runs/${id}/cancel" >/dev/null; then
    echo "cancelled ${WORKFLOW} run ${id} (older head of pull request #${PR_NUMBER})"
  else
    echo "::notice title=Superseded runs::could not cancel run ${id}"
  fi
done <<<"$ids"
