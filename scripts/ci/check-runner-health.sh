#!/usr/bin/env bash
# Sample GitHub control-plane runner registration and label-specific demand.
# Emits runner_state + alert_required. Does not claim host functionality or SLA.
# expected_offline is not health: never publish healthy=true for that state.
set -euo pipefail

repo="${GITHUB_REPOSITORY:-${REPO:-}}"
runner_name="${RUNNER_NAME:-assay-bpf-runner}"
required_runner_label="${REQUIRED_RUNNER_LABEL:-assay-bpf-runner}"
runner_status_token="${RUNNER_STATUS_TOKEN:-${GH_TOKEN:-}}"
queue_token="${QUEUE_TOKEN:-${GH_TOKEN:-}}"
output_file="${GITHUB_OUTPUT:-/dev/null}"
summary_file="${GITHUB_STEP_SUMMARY:-/dev/null}"
tmpdir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

if [[ -z "$repo" ]]; then
  echo "ERROR: GITHUB_REPOSITORY or REPO is required" >&2
  exit 2
fi

if [[ -z "$runner_status_token" || -z "$queue_token" ]]; then
  echo "ERROR: RUNNER_STATUS_TOKEN/QUEUE_TOKEN or GH_TOKEN is required" >&2
  exit 2
fi

write_output() {
  local key="$1"
  local value="$2"
  printf '%s=%s\n' "$key" "$value" >> "$output_file"
}

sanitize_error() {
  tr '\n' ' ' | sed 's/[[:space:]]\+/ /g'
}

count_nonempty_lines() {
  sed '/^[[:space:]]*$/d' | wc -l | tr -d ' '
}

runner_status="unknown"
runner_status_error=""
runner_busy="unknown"
runner_labels=""

runner_status_stderr="$tmpdir/runner-status.stderr"
runner_json="$tmpdir/runners.json"
if GH_TOKEN="$runner_status_token" gh api --paginate "repos/$repo/actions/runners?per_page=100" >"$runner_json" 2>"$runner_status_stderr"; then
  selected_runner="$(jq -sc --arg name "$runner_name" '[.[].runners[]? | select(.name == $name)] | .[0] // empty' "$runner_json")"
  if [[ -z "$selected_runner" ]]; then
    runner_status="not_found"
  else
    runner_status="$(jq -r '.status // "unknown"' <<<"$selected_runner")"
    runner_busy="$(jq -r 'if has("busy") then (.busy | tostring) else "unknown" end' <<<"$selected_runner")"
    runner_labels="$(jq -r '[.labels[]?.name] | join(",")' <<<"$selected_runner")"
  fi
else
  runner_status_error="$(sanitize_error <"$runner_status_stderr")"
  echo "::warning::Unable to query self-hosted runner status: ${runner_status_error}" >&2
fi

matching_jobs="$tmpdir/matching-jobs.tsv"
: >"$matching_jobs"
queue_stderr="$tmpdir/queue.stderr"
: >"$queue_stderr"
queue_status_error=""
general_queued_runs=0
inspected_workflow_runs=0

for run_status in queued in_progress; do
  runs_json="$tmpdir/runs-${run_status}.json"
  if GH_TOKEN="$queue_token" gh api --paginate "repos/$repo/actions/runs?status=${run_status}&per_page=100" >"$runs_json" 2>>"$queue_stderr"; then
    run_rows="$(jq -r '.workflow_runs[]? | [.id, .name, .status, .html_url] | @tsv' "$runs_json")"
    if [[ "$run_status" == "queued" ]]; then
      general_queued_runs="$(jq -r '.workflow_runs[]?.id' "$runs_json" | count_nonempty_lines)"
    fi

    while IFS=$'\t' read -r run_id run_name _status run_url; do
      [[ -n "${run_id:-}" ]] || continue
      inspected_workflow_runs=$((inspected_workflow_runs + 1))

      jobs_json="$tmpdir/jobs-${run_id}.json"
      if GH_TOKEN="$queue_token" gh api --paginate "repos/$repo/actions/runs/${run_id}/jobs?filter=latest&per_page=100" >"$jobs_json" 2>>"$queue_stderr"; then
        jq -r \
          --arg label "$required_runner_label" \
          --arg run_name "$run_name" \
          --arg run_url "$run_url" \
          '
          .jobs[]?
          | select((.status == "queued" or .status == "waiting" or .status == "pending" or .status == "requested")
              and ((.labels // []) | index($label)))
          | [$run_name, .name, .status, ((.labels // []) | join(",")), (.html_url // $run_url)]
          | @tsv
          ' "$jobs_json" >>"$matching_jobs"
      else
        echo "Unable to query jobs for run ${run_id}" >>"$queue_stderr"
      fi
    done <<<"$run_rows"
  fi
done

if [[ -s "$queue_stderr" ]]; then
  queue_status_error="$(sanitize_error <"$queue_stderr")"
  echo "::warning::Unable to fully classify queued jobs: ${queue_status_error}" >&2
fi

matching_queued_jobs="$(count_nonempty_lines <"$matching_jobs")"

# Explicit state contract (issue #2256). Do not overload healthy for offline.
runner_state="classification_unknown"
alert_required="true"
healthy="false"
health_reason="runner_classification_unknown"

if [[ -n "$runner_status_error" ]]; then
  runner_state="classification_unknown"
  alert_required="true"
  healthy="false"
  health_reason="runner_status_classification_failed"
elif [[ "$runner_status" == "online" ]]; then
  runner_state="available"
  alert_required="false"
  healthy="true"
  health_reason="runner_online"
elif [[ "$runner_status" == "not_found" ]]; then
  runner_state="unavailable"
  alert_required="true"
  healthy="false"
  health_reason="runner_not_found"
elif [[ -n "$queue_status_error" ]]; then
  runner_state="classification_unknown"
  alert_required="true"
  healthy="false"
  health_reason="runner_${runner_status}_queue_classification_unknown"
elif [[ "$matching_queued_jobs" =~ ^[0-9]+$ && "$matching_queued_jobs" -gt 0 ]]; then
  runner_state="demand_backed_outage"
  alert_required="true"
  healthy="false"
  health_reason="runner_${runner_status}_with_matching_backlog"
elif [[ "$runner_status" == "offline" ]]; then
  runner_state="expected_offline"
  alert_required="false"
  healthy="false"
  health_reason="runner_offline_without_matching_backlog"
else
  runner_state="classification_unknown"
  alert_required="true"
  healthy="false"
  health_reason="runner_${runner_status}_classification_unknown"
fi

write_output "runner_status" "$runner_status"
write_output "runner_status_error" "$runner_status_error"
write_output "runner_busy" "$runner_busy"
write_output "runner_labels" "$runner_labels"
write_output "required_runner_label" "$required_runner_label"
write_output "general_queued_runs" "$general_queued_runs"
write_output "inspected_workflow_runs" "$inspected_workflow_runs"
write_output "matching_queued_jobs" "$matching_queued_jobs"
write_output "queue_status_error" "$queue_status_error"
write_output "runner_state" "$runner_state"
write_output "alert_required" "$alert_required"
write_output "healthy" "$healthy"
write_output "health_reason" "$health_reason"

{
  echo "## Runner control-plane sample"
  echo "- runner: ${runner_name}"
  echo "- runner_status: ${runner_status}"
  echo "- runner_busy: ${runner_busy}"
  echo "- runner_labels: ${runner_labels:-unknown}"
  echo "- required_runner_label: ${required_runner_label}"
  echo "- general_queued_workflow_runs: ${general_queued_runs}"
  echo "- inspected_workflow_runs: ${inspected_workflow_runs}"
  echo "- matching_queued_jobs: ${matching_queued_jobs}"
  echo "- runner_state: ${runner_state}"
  echo "- alert_required: ${alert_required}"
  echo "- healthy: ${healthy}"
  echo "- health_reason: ${health_reason}"
  echo "- note: samples registration and label demand only; not host functionality or SLA"
  if [[ -n "$runner_status_error" ]]; then
    echo "- runner_status_error: ${runner_status_error}"
  fi
  if [[ -n "$queue_status_error" ]]; then
    echo "- queue_status_error: ${queue_status_error}"
  fi
  if [[ -s "$matching_jobs" ]]; then
    echo ""
    echo "### Matching queued jobs"
    echo '```text'
    cat "$matching_jobs"
    echo '```'
  fi
} >>"$summary_file"

if [[ "$alert_required" == "true" ]]; then
  echo "Runner alert required: state=${runner_state} reason=${health_reason}" >&2
  exit 1
fi

echo "Runner alert not required: state=${runner_state} reason=${health_reason}"
