#!/usr/bin/env bash
# Prove semver checks run from the derived published-lib set, not a hand-maintained allowlist.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/split-wave0-gates.yml"
DERIVER="${ROOT}/scripts/ci/derive-semver-published-lib-crates.py"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

ok() { echo "ok   $*"; }

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

output_value() {
  local file="$1" key="$2"
  awk -F= -v key="${key}" '
    $1 == key { print substr($0, length(key) + 2); found=1; exit }
    END { if (!found) exit 1 }
  ' "${file}"
}

run_detect_case() (
  set -euo pipefail
  local workflow="$1" changed="$2" metadata_override="${3:-}"
  local case_dir detect_run output summary semver unmatched global
  case_dir="$(mktemp -d "${scratch}/detect.XXXXXX")"
  detect_run="${case_dir}/detect.sh"
  step_run "${workflow}" detect-changes '      - id: detect' > "${detect_run}"
  [[ -s "${detect_run}" ]] || fail "could not extract detect run block"

  output="${case_dir}/detect.out"
  summary="${case_dir}/summary.md"
  : > "${output}"
  : > "${summary}"
  mkdir -p "${case_dir}/runner"

  if [[ -n "${metadata_override}" ]]; then
    PATH="${case_dir}/bin:${PATH}" \
    RUNNER_TEMP="${case_dir}/runner" \
    GITHUB_OUTPUT="${output}" \
    GITHUB_STEP_SUMMARY="${summary}" \
    GITHUB_EVENT_NAME=workflow_dispatch \
    SIMULATED_CHANGED_FILES="${changed}" \
    ASSAY_SEMVER_METADATA_JSON="${metadata_override}" \
      bash "${detect_run}"
  else
    RUNNER_TEMP="${case_dir}/runner" \
    GITHUB_OUTPUT="${output}" \
    GITHUB_STEP_SUMMARY="${summary}" \
    GITHUB_EVENT_NAME=workflow_dispatch \
    SIMULATED_CHANGED_FILES="${changed}" \
      bash "${detect_run}"
  fi

  semver="$(output_value "${output}" semver_relevant)"
  unmatched="$(output_value "${output}" unmatched_assay_crate_changed)"
  global="$(output_value "${output}" global_changed)"
  printf '%s\t%s\t%s\n' "${semver}" "${unmatched}" "${global}"
)

build_detect_metadata_fixture() {
  local out="$1"
  cat > "${out}" <<'JSON'
{
  "workspace_members": [
    "assay-common 6.3.0 (path+file:///repo/crates/assay-common)",
    "assay-private 6.3.0 (path+file:///repo/crates/assay-private)",
    "zz-published-lib 0.1.0 (path+file:///repo/crates/zz-published-lib)"
  ],
  "packages": [
    {
      "id": "assay-common 6.3.0 (path+file:///repo/crates/assay-common)",
      "name": "assay-common",
      "source": null,
      "publish": null,
      "manifest_path": "/repo/crates/assay-common/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    },
    {
      "id": "assay-private 6.3.0 (path+file:///repo/crates/assay-private)",
      "name": "assay-private",
      "source": null,
      "publish": [],
      "manifest_path": "/repo/crates/assay-private/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    },
    {
      "id": "zz-published-lib 0.1.0 (path+file:///repo/crates/zz-published-lib)",
      "name": "zz-published-lib",
      "source": null,
      "publish": null,
      "manifest_path": "/repo/crates/zz-published-lib/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    }
  ]
}
JSON
}

assert_detect_routing_contract() {
  local workflow="$1" detect_metadata got semver unmatched global

  got="$(run_detect_case "${workflow}" 'crates/assay-runner-linux/src/lib.rs')"
  semver="${got%%$'\t'*}"
  got="${got#*$'\t'}"
  unmatched="${got%%$'\t'*}"
  global="${got##*$'\t'}"
  [[ "${semver}" == "true" && "${unmatched}" == "true" && "${global}" == "false" ]] \
    || fail "assay-runner-linux case mismatch: semver=${semver} unmatched=${unmatched} global=${global}"

  got="$(run_detect_case "${workflow}" 'crates/assay-sim/src/report.rs')"
  semver="${got%%$'\t'*}"
  got="${got#*$'\t'}"
  unmatched="${got%%$'\t'*}"
  global="${got##*$'\t'}"
  [[ "${semver}" == "true" && "${unmatched}" == "true" && "${global}" == "false" ]] \
    || fail "assay-sim case mismatch: semver=${semver} unmatched=${unmatched} global=${global}"

  got="$(run_detect_case "${workflow}" 'crates/foo-lib/src/lib.rs')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "foo-lib must not flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'Cargo.toml')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "true" ]] || fail "Cargo.toml must flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'Cargo.lock')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "true" ]] || fail "Cargo.lock must flip semver_relevant"

  got="$(run_detect_case "${workflow}" '.github/workflows/split-wave0-gates.yml')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "workflow file must not flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'scripts/ci/derive-semver-published-lib-crates.py')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "deriver file must not flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'scripts/ci/test-semver-gate.sh')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "semver gate self-test file must not flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'scripts/ci/test-split-wave0-semver-routing.sh')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "routing test file must not flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'crates/assay-private/src/lib.rs')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "publish=false crate must not flip semver_relevant"

  got="$(run_detect_case "${workflow}" 'crates/zz-published-lib/src/lib.rs')"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "false" ]] || fail "zz-published-lib must not flip without derivation membership"

  detect_metadata="${scratch}/detect-metadata.json"
  build_detect_metadata_fixture "${detect_metadata}"
  got="$(run_detect_case "${workflow}" 'crates/zz-published-lib/src/lib.rs' "${detect_metadata}")"
  semver="${got%%$'\t'*}"
  [[ "${semver}" == "true" ]] \
    || fail "zz-published-lib must flip when added as publish-unset lib in derived metadata"
}

build_metadata_fixture() {
  local out="$1"
  cat > "${out}" <<'JSON'
{
  "workspace_members": [
    "assay-core 6.3.0 (path+file:///repo/crates/assay-core)",
    "assay-sim 6.3.0 (path+file:///repo/crates/assay-sim)",
    "assay-runner-core 6.3.0 (path+file:///repo/crates/assay-runner-core)",
    "assay-private 6.3.0 (path+file:///repo/crates/assay-private)",
    "assay-cli 6.3.0 (path+file:///repo/crates/assay-cli)"
  ],
  "packages": [
    {
      "id": "assay-core 6.3.0 (path+file:///repo/crates/assay-core)",
      "name": "assay-core",
      "source": null,
      "publish": null,
      "manifest_path": "/repo/crates/assay-core/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    },
    {
      "id": "assay-sim 6.3.0 (path+file:///repo/crates/assay-sim)",
      "name": "assay-sim",
      "source": null,
      "publish": null,
      "manifest_path": "/repo/crates/assay-sim/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    },
    {
      "id": "assay-runner-core 6.3.0 (path+file:///repo/crates/assay-runner-core)",
      "name": "assay-runner-core",
      "source": null,
      "publish": null,
      "manifest_path": "/repo/crates/assay-runner-core/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    },
    {
      "id": "assay-private 6.3.0 (path+file:///repo/crates/assay-private)",
      "name": "assay-private",
      "source": null,
      "publish": [],
      "manifest_path": "/repo/crates/assay-private/Cargo.toml",
      "targets": [{"kind": ["lib"]}]
    },
    {
      "id": "assay-cli 6.3.0 (path+file:///repo/crates/assay-cli)",
      "name": "assay-cli",
      "source": null,
      "publish": null,
      "manifest_path": "/repo/crates/assay-cli/Cargo.toml",
      "targets": [{"kind": ["bin"]}]
    }
  ]
}
JSON
}

assert_derivation_contract() {
  local base_fixture="$1"
  local got expected removed added
  got="$(python3 "${DERIVER}" --metadata-json "${base_fixture}")"
  expected=$'assay-core\tcrates/assay-core/Cargo.toml\nassay-runner-core\tcrates/assay-runner-core/Cargo.toml\nassay-sim\tcrates/assay-sim/Cargo.toml'
  [[ "${got}" == "${expected}" ]] \
    || fail "base derivation mismatch; got:
${got}
expected:
${expected}"
  ok "base derivation selects publishable workspace lib crates"

  removed="${scratch}/metadata-removed.json"
  python3 - "${base_fixture}" "${removed}" <<'PY'
import json
import sys

src, dst = sys.argv[1], sys.argv[2]
data = json.load(open(src))
for package in data["packages"]:
    if package["name"] == "assay-sim":
        package["publish"] = []
json.dump(data, open(dst, "w"), indent=2)
PY
  got="$(python3 "${DERIVER}" --metadata-json "${removed}")"
  if grep -q '^assay-sim\b' <<<"${got}"; then
    fail "publish=false did not remove assay-sim from derived semver set"
  fi
  ok "publish=false drops a crate from derived semver set"

  added="${scratch}/metadata-added.json"
  python3 - "${base_fixture}" "${added}" <<'PY'
import json
import sys

src, dst = sys.argv[1], sys.argv[2]
data = json.load(open(src))
new_id = "assay-newlib 0.1.0 (path+file:///repo/crates/assay-newlib)"
data["workspace_members"].append(new_id)
data["packages"].append(
    {
        "id": new_id,
        "name": "assay-newlib",
        "source": None,
        "publish": None,
        "manifest_path": "/repo/crates/assay-newlib/Cargo.toml",
        "targets": [{"kind": ["lib"]}],
    }
)
json.dump(data, open(dst, "w"), indent=2)
PY
  got="$(python3 "${DERIVER}" --metadata-json "${added}")"
  grep -q $'^assay-newlib\tcrates/assay-newlib/Cargo.toml$' <<<"${got}" \
    || fail "publish-unset new crate did not enter derived semver set; got:
${got}"
  ok "publish-unset new crate enters derived semver set"
}

check_contract() (
  set -euo pipefail
  local workflow="$1" case_dir semver_step semver_run metadata summary cargo_log
  case_dir="$(mktemp -d "${scratch}/case.XXXXXX")"
  semver_step="$(step_block "${workflow}" semver-public '      - name: Run semver checks (allowlist)')"
  semver_run="${case_dir}/semver.sh"
  step_run "${workflow}" semver-public '      - name: Run semver checks (allowlist)' > "${semver_run}"
  [[ -s "${semver_run}" ]] || fail "could not extract semver run block"

  assert_detect_routing_contract "${workflow}"

  grep -q 'derive-semver-published-lib-crates.py' <<<"${semver_step}" \
    || fail "semver step must derive crate set via scripts/ci/derive-semver-published-lib-crates.py"
  if grep -qE 'CORE_CHANGED|REGISTRY_CHANGED|EVIDENCE_CHANGED|COMMON_CHANGED|POLICY_CHANGED|METRICS_CHANGED|RUNNER_SCHEMA_CHANGED' <<<"${semver_step}"; then
    fail "semver step still consumes per-crate routing env vars"
  fi

  metadata="${case_dir}/metadata.json"
  build_metadata_fixture "${metadata}"
  assert_derivation_contract "${metadata}"

  mkdir -p "${case_dir}/bin"
  cat > "${case_dir}/bin/cargo" <<'CARGO'
#!/usr/bin/env bash
printf 'toolchain=%s argv=%s\n' "${RUSTUP_TOOLCHAIN:-}" "$*" >> "${CARGO_LOG}"
CARGO
  chmod +x "${case_dir}/bin/cargo"

  cat > "${case_dir}/bin/git" <<'GIT'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == "cat-file" && "${2:-}" == "-e" ]]; then
  arg="${3:-}"
  if [[ -n "${BASELINE_MISSING_PATTERN:-}" ]] && [[ "${arg}" == *"${BASELINE_MISSING_PATTERN}" ]]; then
    exit 1
  fi
  exit 0
fi
/usr/bin/git "$@"
GIT
  chmod +x "${case_dir}/bin/git"

  summary="${case_dir}/summary.md"
  cargo_log="${case_dir}/cargo.log"
  : > "${summary}"
  : > "${cargo_log}"
  mkdir -p "${case_dir}/runner"

  PATH="${case_dir}/bin:${PATH}" \
  CARGO_LOG="${cargo_log}" \
  RUNNER_TEMP="${case_dir}/runner" \
  GITHUB_STEP_SUMMARY="${summary}" \
  BASELINE_TAG=v-test \
  ASSAY_SEMVER_METADATA_JSON="${metadata}" \
    bash "${semver_run}"

  local expected
  expected=$'toolchain=stable argv=semver-checks check-release -p assay-core --baseline-rev v-test\ntoolchain=stable argv=semver-checks check-release -p assay-runner-core --baseline-rev v-test\ntoolchain=stable argv=semver-checks check-release -p assay-sim --baseline-rev v-test'
  [[ "$(cat "${cargo_log}")" == "${expected}" ]] \
    || fail "semver step did not run the derived crate set; got:
$(cat "${cargo_log}")"
  grep -q 'assay-runner-core' "${summary}" \
    || fail "summary omitted a checked crate from derived set"

  : > "${summary}"
  : > "${cargo_log}"
  PATH="${case_dir}/bin:${PATH}" \
  CARGO_LOG="${cargo_log}" \
  RUNNER_TEMP="${case_dir}/runner" \
  GITHUB_STEP_SUMMARY="${summary}" \
  BASELINE_TAG=v-test \
  BASELINE_MISSING_PATTERN='crates/assay-sim/Cargo.toml' \
  ASSAY_SEMVER_METADATA_JSON="${metadata}" \
    bash "${semver_run}"
  grep -q 'assay-sim (skipped:' "${summary}" \
    || fail "missing-baseline crate did not emit visible skip summary line"
  if grep -q 'assay-sim' "${cargo_log}"; then
    fail "missing-baseline crate still invoked cargo semver-checks"
  fi
)

expect_mutation_to_fail() {
  local name="$1" workflow="$2"
  if check_contract "${workflow}" >/dev/null 2>"${scratch}/${name}.err"; then
    fail "${name} mutation survived"
  fi
  ok "${name} mutation bites"
}

[[ -f "${DERIVER}" ]] || fail "missing ${DERIVER#${ROOT}/}"
check_contract "${WORKFLOW}"

cp "${WORKFLOW}" "${scratch}/missing-deriver.yml"
sed -i.bak '/derive-semver-published-lib-crates\.py/d' "${scratch}/missing-deriver.yml"
expect_mutation_to_fail missing-deriver "${scratch}/missing-deriver.yml"

cp "${WORKFLOW}" "${scratch}/control.yml"
printf '\n# comment-only control\n' >> "${scratch}/control.yml"
check_contract "${scratch}/control.yml"
ok "comment-only control remains green"
echo "split-wave0 semver routing: PASS"
