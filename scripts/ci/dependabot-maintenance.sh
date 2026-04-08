#!/usr/bin/env bash
set -euo pipefail

repo="${1:-${GITHUB_REPOSITORY:-}}"
if [[ -z "$repo" ]]; then
  echo "usage: $0 <owner/repo>" >&2
  exit 2
fi

summary_file="${GITHUB_STEP_SUMMARY:-}"

append_summary() {
  if [[ -n "$summary_file" ]]; then
    printf '%s\n' "$1" >> "$summary_file"
  fi
}

if ! gh auth status >/dev/null 2>&1; then
  echo "gh authentication is required" >&2
  exit 1
fi

resolve_merge_state() {
  local number="$1"
  local state="UNKNOWN"
  for _ in 1 2 3 4; do
    state="$(
      gh pr view "$number" \
        --repo "$repo" \
        --json mergeStateStatus \
        --jq '.mergeStateStatus'
    )"

    if [[ "$state" != "UNKNOWN" ]]; then
      printf '%s\n' "$state"
      return 0
    fi

    sleep 3
  done

  printf '%s\n' "$state"
}

dependabot_refresh_action() {
  local number="$1"

  gh pr view "$number" \
    --repo "$repo" \
    --json commits \
    --jq '
      [.commits[].messageHeadline] as $heads |
      if ($heads | length) <= 1 then
        "rebase"
      elif ($heads[1:] | all(startswith("Merge branch '\''main'\'' into "))) then
        "recreate"
      else
        "rebase"
      end
    '
}

dependabot_refresh_marker() {
  local action="$1"
  local head_sha="$2"
  printf '<!-- dependabot-maintenance:action=%s head=%s -->' "$action" "$head_sha"
}

dependabot_refresh_already_requested() {
  local number="$1"
  local marker="$2"

  gh pr view "$number" \
    --repo "$repo" \
    --json comments \
    --jq --arg marker "$marker" '[.comments[].body | contains($marker)] | any'
}

request_dependabot_refresh() {
  local number="$1"
  local head_sha="$2"
  local action marker body

  action="$(dependabot_refresh_action "$number")"
  marker="$(dependabot_refresh_marker "$action" "$head_sha")"

  if [[ "$(dependabot_refresh_already_requested "$number" "$marker")" == "true" ]]; then
    printf 'already-requested:%s\n' "$action"
    return 0
  fi

  body="@dependabot $action"$'\n\n'"$marker"
  if gh pr comment "$number" --repo "$repo" --body "$body" >/dev/null; then
    printf 'requested:%s\n' "$action"
    return 0
  fi

  printf 'request-failed:%s\n' "$action"
  return 1
}

dependabot_prs="$(
  gh pr list \
    --repo "$repo" \
    --state open \
    --author "app/dependabot" \
    --json number,title,mergeStateStatus,isDraft,autoMergeRequest,url,author,headRefOid
)"

replacement_prs="$(
  gh pr list \
    --repo "$repo" \
    --state open \
    --search "head:codex/dependabot- state:open" \
    --json number,title,mergeStateStatus,isDraft,autoMergeRequest,url,author,headRefOid
)"

prs_json="$(jq -s 'add | unique_by(.number)' <<<"$dependabot_prs"$'\n'"$replacement_prs")"

count="$(jq 'length' <<<"$prs_json")"
append_summary "## Dependency queue maintenance"
append_summary "- repo: $repo"
append_summary "- open_dependency_queue_prs: $count"

if [[ "$count" == "0" ]]; then
  append_summary "- result: no open dependency queue PRs"
  exit 0
fi

updated=0
update_failures=0
dependabot_refresh_requests=0
dependabot_refresh_failures=0
automerge_enabled=0
automerge_failures=0

while IFS=$'\t' read -r number title merge_state is_draft auto_enabled author_login head_sha; do
  resolved_merge_state="$merge_state"
  update_status="not-needed"
  auto_status="already-enabled"

  if [[ "$merge_state" == "UNKNOWN" ]]; then
    resolved_merge_state="$(resolve_merge_state "$number")"
  fi

  if [[ "$resolved_merge_state" == "BEHIND" ]]; then
    if [[ "$author_login" == "app/dependabot" || "$author_login" == "dependabot[bot]" ]]; then
      refresh_result="$(request_dependabot_refresh "$number" "$head_sha")"
      case "$refresh_result" in
        requested:*)
          dependabot_refresh_requests=$((dependabot_refresh_requests + 1))
          update_status="$refresh_result"
          ;;
        already-requested:*)
          update_status="$refresh_result"
          ;;
        *)
          dependabot_refresh_failures=$((dependabot_refresh_failures + 1))
          update_status="$refresh_result"
          ;;
      esac
    else
      if gh pr update-branch "$number" --repo "$repo" >/dev/null; then
        updated=$((updated + 1))
        update_status="updated"
      else
        update_failures=$((update_failures + 1))
        update_status="update-failed"
      fi
    fi
  fi

  if [[ "$is_draft" != "true" && "$auto_enabled" != "true" ]]; then
    if gh pr merge "$number" --repo "$repo" --merge --auto >/dev/null; then
      automerge_enabled=$((automerge_enabled + 1))
      auto_status="enabled"
    else
      automerge_failures=$((automerge_failures + 1))
      auto_status="enable-failed"
    fi
  elif [[ "$is_draft" == "true" ]]; then
    auto_status="draft"
  fi

  append_summary "- PR #$number: $title | merge_state=$merge_state -> $resolved_merge_state | branch=$update_status | auto_merge=$auto_status"
  echo "PR #$number ($title): merge_state=$merge_state -> $resolved_merge_state, branch=$update_status, auto_merge=$auto_status"
done < <(
  jq -r '.[] | [.number, .title, .mergeStateStatus, .isDraft, (.autoMergeRequest != null), .author.login, .headRefOid] | @tsv' <<<"$prs_json"
)

append_summary "- updated_branches: $updated"
append_summary "- update_failures: $update_failures"
append_summary "- dependabot_refresh_requests: $dependabot_refresh_requests"
append_summary "- dependabot_refresh_failures: $dependabot_refresh_failures"
append_summary "- auto_merge_enabled: $automerge_enabled"
append_summary "- auto_merge_failures: $automerge_failures"

if [[ "$update_failures" -gt 0 || "$dependabot_refresh_failures" -gt 0 || "$automerge_failures" -gt 0 ]]; then
  append_summary "- note: some PRs could not be updated automatically; inspect workflow logs for details"
fi
