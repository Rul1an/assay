#!/usr/bin/env bash
set -euo pipefail

: "${REPO:?REPO is required}"
: "${GITHUB_OUTPUT:?GITHUB_OUTPUT is required}"
: "${EXPECTED_BASE_SHA:?EXPECTED_BASE_SHA is required}"

BRANCH="${BRANCH:-docs/auto-update}"
BASE_BRANCH="${BASE_BRANCH:-main}"
MAX_ATTEMPTS="${MAX_ATTEMPTS:-12}"
RETRY_SECONDS="${RETRY_SECONDS:-5}"

write_output() {
  printf '%s=%s\n' "$1" "$2" >> "$GITHUB_OUTPUT"
}

live_base_sha() {
  gh api "repos/${REPO}/commits/${BASE_BRANCH}" --jq '.sha'
}

observed_base="$(live_base_sha)"
if [[ "$observed_base" != "$EXPECTED_BASE_SHA" ]]; then
  echo "main advanced from ${EXPECTED_BASE_SHA} to ${observed_base}; refusing stale docs generation" >&2
  exit 2
fi

pr_json="$(
  gh pr list \
    --repo "$REPO" \
    --state open \
    --head "$BRANCH" \
    --base "$BASE_BRANCH" \
    --json number
)"

pr_count="$(jq 'length' <<<"$pr_json")"
if [[ "$pr_count" -eq 0 ]]; then
  write_output pr_number ""
  write_output head_sha ""
  write_output branch_updated false
  echo "No open ${BRANCH} pull request."
  exit 0
fi

if [[ "$pr_count" -ne 1 ]]; then
  echo "expected exactly one open docs PR for ${BRANCH}; found ${pr_count}" >&2
  exit 2
fi

pr_number="$(jq -r '.[0].number' <<<"$pr_json")"
repo_owner="${REPO%%/*}"
initial_head=""
head_sha=""
merge_state="UNKNOWN"
update_requested=false
last_requested_head=""
stable=false

for ((attempt = 1; attempt <= MAX_ATTEMPTS; attempt++)); do
  current="$(
    gh pr view "$pr_number" \
      --repo "$REPO" \
      --json \
author,baseRefName,baseRefOid,headRefName,headRefOid,headRepositoryOwner,isCrossRepository,mergeStateStatus
  )"

  author_login="$(jq -r '.author.login' <<<"$current")"
  base_ref="$(jq -r '.baseRefName' <<<"$current")"
  base_sha="$(jq -r '.baseRefOid' <<<"$current")"
  head_ref="$(jq -r '.headRefName' <<<"$current")"
  head_sha="$(jq -r '.headRefOid' <<<"$current")"
  head_owner="$(jq -r '.headRepositoryOwner.login' <<<"$current")"
  cross_repo="$(jq -r '.isCrossRepository' <<<"$current")"
  merge_state="$(jq -r '.mergeStateStatus' <<<"$current")"

  if [[ "$author_login" != "app/github-actions" ||
        "$base_ref" != "$BASE_BRANCH" ||
        "$head_ref" != "$BRANCH" ||
        "$head_owner" != "$repo_owner" ||
        "$cross_repo" != "false" ]]; then
    echo "docs PR #${pr_number} has unexpected author, repository, or branch identity" >&2
    exit 2
  fi
  if [[ -z "$head_sha" || "$head_sha" == "null" ]]; then
    echo "docs PR #${pr_number} is missing its head SHA" >&2
    exit 2
  fi
  [[ -n "$initial_head" ]] || initial_head="$head_sha"

  case "$merge_state" in
    CLEAN | BLOCKED | UNSTABLE | HAS_HOOKS)
      if [[ "$base_sha" != "$EXPECTED_BASE_SHA" ]]; then
        echo "docs PR #${pr_number} is stable on base ${base_sha}, expected ${EXPECTED_BASE_SHA}" >&2
        exit 2
      fi
      stable=true
      break
      ;;
    BEHIND)
      if [[ "$head_sha" != "$last_requested_head" ]]; then
        gh api \
          --method PUT \
          "repos/${REPO}/pulls/${pr_number}/update-branch" \
          -f "expected_head_sha=${head_sha}" >/dev/null
        update_requested=true
        last_requested_head="$head_sha"
      fi
      ;;
    UNKNOWN)
      ;;
    DIRTY)
      echo "docs PR #${pr_number} has merge state DIRTY; refusing automatic update" >&2
      exit 2
      ;;
    *)
      echo "docs PR #${pr_number} has unsupported merge state ${merge_state}" >&2
      exit 2
      ;;
  esac

  sleep "$RETRY_SECONDS"
done

if [[ "$stable" != "true" ]]; then
  echo "docs PR #${pr_number} did not reach a stable merge state" >&2
  exit 2
fi
if [[ "$update_requested" == "true" && "$head_sha" == "$initial_head" ]]; then
  echo "docs PR #${pr_number} did not advance from ${initial_head}" >&2
  exit 2
fi

files_json="$(
  gh api \
    --paginate \
    --slurp \
    "repos/${REPO}/pulls/${pr_number}/files?per_page=100"
)"
if ! jq -e '
  type == "array" and
  length > 0 and
  all(.[]; type == "array") and
  ([.[][]] | length > 0)
' >/dev/null <<<"$files_json"; then
  echo "docs PR #${pr_number} returned an invalid or empty files response" >&2
  exit 2
fi

non_docs="$(jq -c '[.[][] | .filename | select(startswith("docs/") | not)]' <<<"$files_json")"
if [[ "$non_docs" != "[]" ]]; then
  echo "docs PR #${pr_number} contains non-doc paths: ${non_docs}" >&2
  exit 2
fi

final_state="$(
  gh pr view "$pr_number" \
    --repo "$REPO" \
    --json baseRefOid,headRefOid
)"
final_head="$(jq -r '.headRefOid' <<<"$final_state")"
final_base="$(jq -r '.baseRefOid' <<<"$final_state")"
if [[ "$final_head" != "$head_sha" ]]; then
  echo "docs PR #${pr_number} moved from ${head_sha} to ${final_head} during validation" >&2
  exit 2
fi
if [[ "$final_base" != "$EXPECTED_BASE_SHA" ]]; then
  echo "docs PR #${pr_number} ended on base ${final_base}, expected ${EXPECTED_BASE_SHA}" >&2
  exit 2
fi
observed_base="$(live_base_sha)"
if [[ "$observed_base" != "$EXPECTED_BASE_SHA" ]]; then
  echo "main advanced from ${EXPECTED_BASE_SHA} to ${observed_base} during validation" >&2
  exit 2
fi

write_output pr_number "$pr_number"
write_output head_sha "$head_sha"
write_output branch_updated "$update_requested"

echo "Docs PR #${pr_number}: head=${head_sha} state=${merge_state} updated=${update_requested}"
