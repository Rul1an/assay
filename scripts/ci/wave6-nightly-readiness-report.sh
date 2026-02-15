#!/usr/bin/env bash
set -euo pipefail

repo="${REPO:-${GITHUB_REPOSITORY:-}}"
workflow_file="wave6-nightly-safety.yml"
window_runs=20
min_runs=14
min_days=14
output_json="nightly_readiness_report.json"
output_md="nightly_readiness_report.md"

while [ $# -gt 0 ]; do
  case "$1" in
    --repo)
      repo="$2"
      shift 2
      ;;
    --workflow-file)
      workflow_file="$2"
      shift 2
      ;;
    --window)
      window_runs="$2"
      shift 2
      ;;
    --min-runs)
      min_runs="$2"
      shift 2
      ;;
    --min-days)
      min_days="$2"
      shift 2
      ;;
    --output-json)
      output_json="$2"
      shift 2
      ;;
    --output-md)
      output_md="$2"
      shift 2
      ;;
    *)
      echo "unknown argument: $1"
      exit 1
      ;;
  esac
done

if [ -z "${repo}" ]; then
  echo "repo is required (use --repo or REPO/GITHUB_REPOSITORY)"
  exit 1
fi

token="${GITHUB_TOKEN:-${GH_TOKEN:-}}"
if [ -z "${token}" ]; then
  echo "GITHUB_TOKEN or GH_TOKEN is required"
  exit 1
fi

api_get() {
  local url="$1"
  curl -fsSL \
    -H "Authorization: Bearer ${token}" \
    -H "Accept: application/vnd.github+json" \
    "${url}"
}

readiness_tmp="$(mktemp)"
trap 'rm -f "${readiness_tmp}"' EXIT

runs_url="https://api.github.com/repos/${repo}/actions/workflows/${workflow_file}/runs?branch=main&event=schedule&status=completed&per_page=100"
runs_json="$(api_get "${runs_url}")"

selected_runs="$(echo "${runs_json}" | jq -c --argjson n "${window_runs}" '
  (.workflow_runs // [])
  | sort_by(.created_at)
  | reverse
  | .[:$n]
')"

echo "${selected_runs}" | jq -c '.[]' | while IFS= read -r run; do
  run_id="$(echo "${run}" | jq -r '.id')"
  jobs_url="https://api.github.com/repos/${repo}/actions/runs/${run_id}/jobs?per_page=100"
  jobs_json="$(api_get "${jobs_url}")"

  jq -n \
    --argjson run "${run}" \
    --argjson jobs "${jobs_json}" \
    '
    def classify($c; $attempt):
      if ($c == "success") and ($attempt > 1) then "flake"
      elif $c == "success" then "success"
      elif ($c == "cancelled" or $c == "timed_out") then "infra"
      elif $c == "failure" then "test"
      else "infra"
      end;

    def duration_seconds($s; $e):
      if (($s | type) == "string") and (($e | type) == "string") then
        ((($e | fromdateiso8601) - ($s | fromdateiso8601)) | floor)
      else
        0
      end;

    def smoke_name($n):
      ($n == "Nightly Miri (assay-registry smoke)")
      or ($n == "Nightly property smoke (assay-cli)");

    def run_category($smoke_jobs; $attempt):
      ($smoke_jobs | map((.conclusion // "unknown") as $c | classify($c; $attempt))) as $cats
      | if ($cats | index("test")) then "test"
        elif ($cats | index("infra")) then "infra"
        elif ($cats | index("flake")) then "flake"
        elif (($cats | length) > 0) and (($cats | map(select(. == "success")) | length) == ($cats | length)) then "success"
        else "infra"
        end;

    ($run.run_attempt // 1) as $attempt
    | (($jobs.jobs // []) | map(select(smoke_name(.name)))) as $smoke_jobs
    | {
        run_id: $run.id,
        run_attempt: $attempt,
        conclusion: ($run.conclusion // "unknown"),
        created_at: $run.created_at,
        run_started_at: $run.run_started_at,
        updated_at: $run.updated_at,
        duration_seconds: duration_seconds($run.run_started_at; $run.updated_at),
        category: run_category($smoke_jobs; $attempt),
        smoke_jobs: ($smoke_jobs | map({
          job_id: .id,
          name: .name,
          conclusion: (.conclusion // "unknown")
        }))
      }
    ' >> "${readiness_tmp}"
done

report_json="$(jq -n \
  --arg repo "${repo}" \
  --arg workflow_file "${workflow_file}" \
  --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  --argjson min_runs "${min_runs}" \
  --argjson min_days "${min_days}" \
  --argjson runs "$(jq -s '.' "${readiness_tmp}")" '
  def median($arr):
    ($arr | sort) as $s
    | if ($s | length) == 0 then null
      elif ($s | length) % 2 == 1 then $s[($s | length / 2 | floor)]
      else (($s[($s | length / 2 - 1)] + $s[($s | length / 2)]) / 2)
      end;

  def p95($arr):
    ($arr | sort) as $s
    | if ($s | length) == 0 then null
      else $s[((($s | length) - 1) * 0.95 | floor)]
      end;

  def max_red_streak($arr):
    (reduce $arr[] as $v ({cur:0,max:0};
      if ($v == true) then
        .cur += 1
        | .max = (if .cur > .max then .cur else .max end)
      else
        .cur = 0
      end
    ) | .max);

  ($runs | length) as $count
  | ($runs[0].created_at // null) as $newest
  | ($runs[-1].created_at // null) as $oldest
  | (if ($newest != null and $oldest != null)
      then (((($newest | fromdateiso8601) - ($oldest | fromdateiso8601)) / 86400))
      else 0
    end) as $span_days
  | ($runs | map(.category == "success") | map(select(. == true)) | length) as $success_runs
  | ($runs | map(.category == "test") | map(select(. == true)) | length) as $test_fail_runs
  | ($runs | map(.category == "infra") | map(select(. == true)) | length) as $infra_fail_runs
  | ($runs | map(.category == "flake") | map(select(. == true)) | length) as $flake_runs
  | ($success_runs + $test_fail_runs + $flake_runs) as $success_denominator
  | (if $success_denominator > 0 then ($success_runs / $success_denominator) else 0 end) as $success_rate_excl_infra
  | (if $count > 0 then ($infra_fail_runs / $count) else 0 end) as $infra_failure_rate
  | (if $count > 0 then ($flake_runs / $count) else 0 end) as $flake_rate
  | ($runs | map(.duration_seconds / 60)) as $duration_minutes
  | (median($duration_minutes)) as $duration_median_minutes
  | (p95($duration_minutes)) as $duration_p95_minutes
  | ($runs | .[:10] | map((.category == "test") or (.category == "flake"))) as $recent_red_flags
  | (max_red_streak($recent_red_flags)) as $recent_red_streak
  | ($count >= $min_runs) as $count_ok
  | ($span_days >= $min_days) as $span_ok
  | ($success_rate_excl_infra >= 0.95) as $success_ok
  | ($infra_failure_rate <= 0.10) as $infra_ok
  | ($flake_rate <= 0.05) as $flake_ok
  | ((($duration_median_minutes // 9999) <= 20)) as $median_ok
  | ((($duration_p95_minutes // 9999) <= 35)) as $p95_ok
  | ($recent_red_streak <= 1) as $streak_ok
  | {
      schema_version: 1,
      classifier_version: 1,
      repo: $repo,
      workflow_file: $workflow_file,
      generated_at_utc: $generated_at,
      policy: {
        window_runs: 20,
        min_runs: $min_runs,
        min_days: $min_days,
        thresholds: {
          smoke_success_rate_excl_infra_min: 0.95,
          infra_failure_rate_max: 0.10,
          flake_rate_max: 0.05,
          duration_median_minutes_max: 20,
          duration_p95_minutes_max: 35,
          recent_red_streak_max: 1
        },
        flake_rule: "run_attempt > 1 only"
      },
      metrics: {
        scheduled_runs_count: $count,
        window_span_days: $span_days,
        smoke_success_rate_excl_infra: $success_rate_excl_infra,
        infra_failure_rate: $infra_failure_rate,
        flake_rate: $flake_rate,
        duration_median_minutes: $duration_median_minutes,
        duration_p95_minutes: $duration_p95_minutes,
        recent_red_streak: $recent_red_streak
      },
      checks: {
        count_ok: $count_ok,
        span_ok: $span_ok,
        success_ok: $success_ok,
        infra_ok: $infra_ok,
        flake_ok: $flake_ok,
        median_ok: $median_ok,
        p95_ok: $p95_ok,
        streak_ok: $streak_ok
      },
      promotion_ready: ($count_ok and $span_ok and $success_ok and $infra_ok and $flake_ok and $median_ok and $p95_ok and $streak_ok),
      runs: $runs
    }
')"

echo "${report_json}" > "${output_json}"

promotion_ready="$(echo "${report_json}" | jq -r '.promotion_ready')"
count="$(echo "${report_json}" | jq -r '.metrics.scheduled_runs_count')"
span_days="$(echo "${report_json}" | jq -r '.metrics.window_span_days')"
success_rate="$(echo "${report_json}" | jq -r '.metrics.smoke_success_rate_excl_infra')"
infra_rate="$(echo "${report_json}" | jq -r '.metrics.infra_failure_rate')"
flake_rate="$(echo "${report_json}" | jq -r '.metrics.flake_rate')"
median_m="$(echo "${report_json}" | jq -r '.metrics.duration_median_minutes')"
p95_m="$(echo "${report_json}" | jq -r '.metrics.duration_p95_minutes')"
streak="$(echo "${report_json}" | jq -r '.metrics.recent_red_streak')"

cat > "${output_md}" <<EOF
## Wave6 Nightly Promotion Readiness

- promotion_ready: \`${promotion_ready}\`
- scheduled_runs_count: \`${count}\`
- window_span_days: \`${span_days}\`
- smoke_success_rate_excl_infra: \`${success_rate}\`
- infra_failure_rate: \`${infra_rate}\`
- flake_rate: \`${flake_rate}\`
- duration_median_minutes: \`${median_m}\`
- duration_p95_minutes: \`${p95_m}\`
- recent_red_streak: \`${streak}\`

Policy:
- flake rule: \`run_attempt > 1\` only
- no required-check changes in Step4
EOF

echo "wrote ${output_json}"
echo "wrote ${output_md}"
