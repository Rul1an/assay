#!/usr/bin/env bash
# Prove that an assay-runner-schema change reaches its cargo-semver-checks invocation.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/split-wave0-gates.yml"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

job_block() {
  local workflow="$1" job="$2"
  awk -v heading="  ${job}:" '
    $0 == heading { in_job=1; print; next }
    in_job && /^  [A-Za-z0-9_-]+:[[:space:]]*$/ { exit }
    in_job { print }
  ' "${workflow}"
}

step_block() {
  local workflow="$1" job="$2" marker="$3"
  job_block "${workflow}" "${job}" | awk -v marker="${marker}" '
    $0 == marker { in_step=1; print; next }
    in_step && /^      - / { exit }
    in_step { print }
  '
}

step_run() {
  local workflow="$1" job="$2" marker="$3"
  step_block "${workflow}" "${job}" "${marker}" | awk '
    /^[[:space:]]*run:[[:space:]]*\|[-+]?[[:space:]]*$/ { in_run=1; next }
    in_run && /^          / { sub(/^          /, ""); print; next }
    in_run && /^$/ { print; next }
    in_run { exit }
  '
}

require_one_line() {
  local text="$1" line="$2" description="$3" count
  count="$(printf '%s\n' "${text}" | grep -Fxc -- "${line}" || true)"
  [[ "${count}" -eq 1 ]] \
    || fail "${description}: expected one active line, found ${count}"
}

output_value() {
  local output_file="$1" key="$2"
  awk -F= -v key="${key}" '$1 == key { value=substr($0, length(key) + 2) } END { print value }' \
    "${output_file}"
}

check_contract() (
  set -euo pipefail
  local workflow="$1" case_dir detector detector_job semver_step
  case_dir="$(mktemp -d "${scratch}/case.XXXXXX")"
  detector="${case_dir}/detect.sh"
  detector_job="$(job_block "${workflow}" detect-changes)"
  semver_step="$(step_block "${workflow}" semver-public '      - name: Run semver checks (allowlist)')"

  step_run "${workflow}" detect-changes '      - id: detect' > "${detector}"
  [[ -s "${detector}" ]] || fail "could not extract detect step run block"
  mkdir -p "${case_dir}/runner"
  : > "${case_dir}/outputs"
  : > "${case_dir}/summary"
  RUNNER_TEMP="${case_dir}/runner" \
  GITHUB_OUTPUT="${case_dir}/outputs" \
  GITHUB_STEP_SUMMARY="${case_dir}/summary" \
  GITHUB_EVENT_NAME=workflow_dispatch \
  SIMULATED_CHANGED_FILES=crates/assay-runner-schema/src/lib.rs \
    bash "${detector}"

  [[ "$(output_value "${case_dir}/outputs" assay_runner_schema_changed)" == "true" ]] \
    || fail "assay-runner-schema change did not emit assay_runner_schema_changed=true"
  [[ "$(output_value "${case_dir}/outputs" semver_relevant)" == "true" ]] \
    || fail "assay-runner-schema change did not emit semver_relevant=true"
  [[ "$(output_value "${case_dir}/outputs" unmatched_assay_crate_changed)" == "false" ]] \
    || fail "assay-runner-schema remains classified as an unmatched Assay crate"

  require_one_line "${detector_job}" \
    "      assay_runner_schema_changed: \${{ steps.detect.outputs.assay_runner_schema_changed }}" \
    "detect output is not projected to the job"
  require_one_line "${semver_step}" \
    "          RUNNER_SCHEMA_CHANGED: \${{ needs.detect-changes.outputs.assay_runner_schema_changed }}" \
    "runner-schema job output is not consumed by the semver step"

  step_run "${workflow}" semver-public '      - name: Run semver checks (allowlist)' \
    > "${case_dir}/semver.sh"
  [[ -s "${case_dir}/semver.sh" ]] || fail "could not extract semver run block"
  mkdir -p "${case_dir}/bin"
  cat > "${case_dir}/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
printf 'toolchain=%s argv=%s\n' "${RUSTUP_TOOLCHAIN:-}" "$*" >> "${CARGO_LOG}"
CARGO
  chmod +x "${case_dir}/bin/cargo"
  : > "${case_dir}/cargo.log"

  PATH="${case_dir}/bin:${PATH}" \
  CARGO_LOG="${case_dir}/cargo.log" \
  GITHUB_STEP_SUMMARY="${case_dir}/summary" \
  BASELINE_TAG=test-baseline \
  RUN_ALL=false \
  GLOBAL_CHANGED=false \
  CORE_CHANGED=false \
  REGISTRY_CHANGED=false \
  EVIDENCE_CHANGED=false \
  COMMON_CHANGED=false \
  POLICY_CHANGED=false \
  METRICS_CHANGED=false \
  RUNNER_SCHEMA_CHANGED=true \
    bash "${case_dir}/semver.sh"

  expected='toolchain=stable argv=semver-checks check-release -p assay-runner-schema --baseline-rev test-baseline'
  [[ "$(cat "${case_dir}/cargo.log")" == "${expected}" ]] \
    || fail "runner-schema route did not make exactly the expected cargo invocation; got: $(cat "${case_dir}/cargo.log")"
)

expect_mutation_to_fail() {
  local name="$1" workflow="$2"
  if check_contract "${workflow}" >/dev/null 2>"${scratch}/${name}.err"; then
    fail "${name} mutation survived"
  fi
  echo "ok   ${name} mutation bites"
}

check_contract "${WORKFLOW}"

cp "${WORKFLOW}" "${scratch}/missing-job-output.yml"
sed -i.bak '/^[[:space:]]*assay_runner_schema_changed: \${{ steps\.detect\.outputs\.assay_runner_schema_changed }}[[:space:]]*$/d' \
  "${scratch}/missing-job-output.yml"
expect_mutation_to_fail missing-job-output "${scratch}/missing-job-output.yml"

cp "${WORKFLOW}" "${scratch}/missing-step-env.yml"
sed -i.bak '/^[[:space:]]*RUNNER_SCHEMA_CHANGED: \${{ needs\.detect-changes\.outputs\.assay_runner_schema_changed }}[[:space:]]*$/d' \
  "${scratch}/missing-step-env.yml"
expect_mutation_to_fail missing-step-env "${scratch}/missing-step-env.yml"

cp "${WORKFLOW}" "${scratch}/missing-invocation.yml"
sed -i.bak 's/^[[:space:]]*run_semver_for assay-runner-schema[[:space:]]*$/            : # runner-schema invocation removed/' \
  "${scratch}/missing-invocation.yml"
expect_mutation_to_fail missing-invocation "${scratch}/missing-invocation.yml"

cp "${WORKFLOW}" "${scratch}/control.yml"
printf '\n# comment-only control\n' >> "${scratch}/control.yml"
check_contract "${scratch}/control.yml"
echo "ok   comment-only control remains green"
echo "split-wave0 semver routing: PASS"
