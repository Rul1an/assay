#!/usr/bin/env bash
# Pins the bounded lint timeout and fail-closed summary in kernel-matrix.yml (#2760).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${1:-${ROOT}/.github/workflows/kernel-matrix.yml}"
SELF_TEST_TMP=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

validate() {
  local workflow="$1"
  [[ -f "${workflow}" ]] || fail "missing workflow ${workflow}"

  ruby -ryaml - "${workflow}" <<'RUBY'
path = ARGV.fetch(0)
doc = YAML.safe_load_file(path, aliases: false)
abort "kernel-matrix.yml must be a mapping" unless doc.is_a?(Hash)

jobs = doc.fetch("jobs")
abort "kernel-matrix.yml jobs must be a mapping" unless jobs.is_a?(Hash)

lint = jobs.fetch("lint")
abort "lint job must be a mapping" unless lint.is_a?(Hash)
timeout = lint["timeout-minutes"]
abort "lint: timeout-minutes must be the measured 15-minute class, got #{timeout.inspect}" unless timeout == 15

summary = jobs.fetch("summary")
abort "summary job must be a mapping" unless summary.is_a?(Hash)
abort "summary must stay fail-closed with if: always()" unless summary["if"] == "always()"
needs = Array(summary["needs"]).map(&:to_s)
abort "summary must consume the lint result" unless needs.include?("lint")

steps = summary.fetch("steps")
abort "summary steps must be an array" unless steps.is_a?(Array)
report = steps.find { |step| step.is_a?(Hash) && step["name"] == "Report Status" }
abort "summary must keep the Report Status step" unless report
env = report.fetch("env")
abort "Report Status env must be a mapping" unless env.is_a?(Hash)
expected_lint_result = "${{ needs.lint.result }}"
abort "Report Status must read needs.lint.result" unless env["LINT_RESULT"] == expected_lint_result
run = report.fetch("run")
abort "Report Status run must be a string" unless run.is_a?(String)
guard = 'if [ "${LINT_RESULT}" != "success" ]; then'
abort "Report Status must fail closed when lint is not successful" unless run.include?(guard)

puts "kernel-matrix timeout contract=passed (lint=15m; summary fail-closed)"
RUBY
}

self_test() {
  validate "${WORKFLOW}" >/dev/null
  SELF_TEST_TMP="$(mktemp -d)"
  trap 'rm -rf -- "${SELF_TEST_TMP:?}"' EXIT

  run_mutation() {
    local name="$1"
    local mutation="$2"
    local candidate="${SELF_TEST_TMP}/${name}.yml"
    cp "${WORKFLOW}" "${candidate}"
    ruby -ryaml - "${candidate}" "${mutation}" <<'RUBY'
path, mutation = ARGV
doc = YAML.safe_load_file(path, aliases: false)
jobs = doc.fetch("jobs")
lint = jobs.fetch("lint")
summary = jobs.fetch("summary")
report = summary.fetch("steps").find { |step| step["name"] == "Report Status" }

case mutation
when "missing-timeout"
  lint.delete("timeout-minutes")
when "short-timeout"
  lint["timeout-minutes"] = 10
when "wrong-timeout-class"
  lint["timeout-minutes"] = 20
when "summary-not-always"
  summary["if"] = "success()"
when "summary-drops-lint"
  summary["needs"] = Array(summary["needs"]).reject { |need| need.to_s == "lint" }
when "summary-reads-wrong-result"
  report.fetch("env")["LINT_RESULT"] = "${{ needs.check-ebpf-changes.result }}"
when "summary-drops-lint-guard"
  report["run"] = report.fetch("run").sub(
    /\n\s*if \[ "\$\{LINT_RESULT\}" != "success" \]; then\n\s*echo "Lint did not complete successfully: \$\{LINT_RESULT\}"\n\s*exit 1\n\s*fi\n/,
    "\n"
  )
else
  abort "unknown mutation #{mutation}"
end

File.write(path, YAML.dump(doc))
RUBY
    if validate "${candidate}" >/dev/null 2>&1; then
      echo "mutation did not bite: ${name}" >&2
      exit 1
    fi
    echo "ok    ${name}"
  }

  run_mutation missing-timeout missing-timeout
  run_mutation short-timeout short-timeout
  run_mutation wrong-timeout-class wrong-timeout-class
  run_mutation summary-not-always summary-not-always
  run_mutation summary-drops-lint summary-drops-lint
  run_mutation summary-reads-wrong-result summary-reads-wrong-result
  run_mutation summary-drops-lint-guard summary-drops-lint-guard
  echo "kernel-matrix timeout contract self-test=passed"
}

if [[ "${2:-}" == "--self-test" || "${1:-}" == "--self-test" ]]; then
  [[ "${1:-}" == "--self-test" ]] && WORKFLOW="${ROOT}/.github/workflows/kernel-matrix.yml"
  self_test
else
  validate "${WORKFLOW}"
fi
