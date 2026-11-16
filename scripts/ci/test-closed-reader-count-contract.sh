#!/usr/bin/env bash
# Contract for where the closed-reader counter runs (ADR-052, decision 5).
#
# The counter only blocks a merge if a job the required `CI` aggregate waits on runs it and can
# fail on it. This pins that: the `clippy` job of ci.yml has one step whose `run` is exactly the
# counter command, with nothing that lets a failure pass, and the aggregate lists `clippy` among
# its needs.
#
# The workflow is parsed, not searched. A copy of the command in a comment, in another step's
# block scalar, or followed by `|| true` is not the step.
#
# `--self-test` runs the same check against copies of the workflow with the step neutralised in
# each way the check is meant to catch, and fails unless every copy is refused for that reason.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
COMMAND="python3 scripts/ci/closed_reader_count.py"

validate() {
  ruby -ryaml - "$1" "${COMMAND}" <<'RUBY'
path, command = ARGV
doc = YAML.safe_load_file(path, aliases: false)
abort "FAIL: ci.yml must be a mapping" unless doc.is_a?(Hash)
jobs = doc.fetch("jobs")
clippy = jobs["clippy"]
abort "FAIL: no `clippy` job" unless clippy.is_a?(Hash)
abort "FAIL: the `clippy` job has continue-on-error" if clippy.key?("continue-on-error")

steps = clippy.fetch("steps")
found = steps.select { |step| step.is_a?(Hash) && step["run"].is_a?(String) && step["run"].strip == command }
abort "FAIL: the `clippy` job has no step whose run is exactly `#{command}`" if found.empty?
abort "FAIL: the `clippy` job runs the counter #{found.size} times" if found.size > 1
step = found.first
allowed = %w[name run]
extra = step.keys - allowed
abort "FAIL: the counter step carries #{extra.join(', ')}; only #{allowed.join(' and ')} are allowed" unless extra.empty?

needs = Array(jobs.fetch("ci").fetch("needs"))
abort "FAIL: the `ci` aggregate does not need `clippy`" unless needs.include?("clippy")
puts "closed-reader-count contract: the clippy job runs `#{command}` and the ci aggregate needs it"
RUBY
}

self_test() {
  local tmp failed=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  # name @@ expected message @@ ruby that edits the parsed workflow
  local cases=(
    "the step is removed@@has no step whose run is exactly@@clippy['steps'].reject! { |s| s.is_a?(Hash) && s['run'].to_s.strip == command }"
    "the command is followed by || true@@has no step whose run is exactly@@step['run'] = command + ' || true'"
    "the command sits in a comment of another line@@has no step whose run is exactly@@step['run'] = '# ' + command + \"\\necho skipped\""
    "the step has if: false@@the counter step carries if@@step['if'] = false"
    "the step has continue-on-error@@the counter step carries continue-on-error@@step['continue-on-error'] = true"
    "the step has a working-directory@@the counter step carries working-directory@@step['working-directory'] = 'docs'"
    "the job has continue-on-error@@job has continue-on-error@@clippy['continue-on-error'] = true"
    "the step runs twice@@runs the counter 2 times@@clippy['steps'] << { 'run' => command }"
    "the aggregate does not need clippy@@aggregate does not need@@doc['jobs']['ci']['needs'] = Array(doc['jobs']['ci']['needs']) - ['clippy']"
    "the step moved to another job@@has no step whose run is exactly@@doc['jobs']['rustdoc']['steps'] << clippy['steps'].delete(step)"
  )

  local entry name rest expected edit out status
  for entry in "${cases[@]}"; do
    name="${entry%%@@*}"
    rest="${entry#*@@}"
    expected="${rest%%@@*}"
    edit="${rest#*@@}"
    ruby -ryaml - "${WORKFLOW}" "${COMMAND}" "${tmp}/ci.yml" "${edit}" <<'RUBY'
path, command, out, edit = ARGV
doc = YAML.safe_load_file(path, aliases: false)
clippy = doc['jobs']['clippy']
step = clippy['steps'].find { |s| s.is_a?(Hash) && s['run'].to_s.strip == command }
abort "self-test: the real workflow has no counter step to neutralise" if step.nil?
eval(edit)
File.write(out, YAML.dump(doc))
RUBY
    set +e
    out="$(validate "${tmp}/ci.yml" 2>&1)"
    status=$?
    set -e
    if [[ ${status} -eq 0 ]]; then
      echo "self-test FAIL: ${name}: the contract passed" >&2
      failed=1
    elif [[ "${out}" != *"${expected}"* ]]; then
      echo "self-test FAIL: ${name}: refused for another reason: ${out}" >&2
      failed=1
    fi
  done

  # The unedited workflow, written the same way, must pass: the edits are what the cases refuse.
  ruby -ryaml -e 'File.write(ARGV[1], YAML.dump(YAML.safe_load_file(ARGV[0], aliases: false)))' "${WORKFLOW}" "${tmp}/ci.yml"
  if ! validate "${tmp}/ci.yml" >/dev/null; then
    echo "self-test FAIL: the unedited workflow, re-serialised, is refused" >&2
    failed=1
  fi

  [[ ${failed} -eq 0 ]] || return 1
  echo "closed-reader-count contract self-test: ${#cases[@]} neutralised workflows refused, each for its own reason"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
else
  validate "${WORKFLOW}"
fi
