#!/usr/bin/env bash
# Contract for where the closed-reader counter runs (ADR-052, decision 5).
#
# The counter only blocks a merge if a job the required `CI` aggregate waits on runs it and can
# fail on it. This pins that: the `clippy` job of ci.yml has one step whose `run` is exactly the
# counter command, and the aggregate lists `clippy` among its needs.
#
# What could let a failing counter pass is not a list that can be finished by naming cases, so
# the rule is a closed shape instead. Everything that decides how a `run` text is executed, or
# what the command in it resolves to, has to be in a set this file names:
#
#   - the workflow has no `defaults`, and its `env` holds only the keys listed below;
#   - the `clippy` job has only the keys listed below, so no `defaults`, `env`, `container`,
#     `continue-on-error` or anything else arrives without this file changing;
#   - a step of that job has only the keys listed below, so no step `env` and no
#     `continue-on-error`; a `shell` is `bash`; and no `run` text writes GITHUB_PATH or GITHUB_ENV,
#     the two files through which one step changes what a later step runs;
#   - the counter's own step has `name` and `run` and nothing else.
#
# The command runs python with `-I`, so no PYTHON* variable and no directory on the path decides
# which modules it imports.
#
# Not pinned: an earlier step that rewrites the script, the ledger or the interpreter on disk. That
# is a change to a `run` text or an action, and it is in the diff a reviewer reads.
#
# The workflow is parsed, not searched. A copy of the command in a comment, in another step's
# block scalar, or followed by `|| true` is not the step.
#
# `--self-test` runs the same check against copies of the workflow, each changed in one way the
# check is meant to refuse, and fails unless every copy is refused for that reason.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
COMMAND="python3 -I scripts/ci/closed_reader_count.py"

validate() {
  ruby -ryaml - "$1" "${COMMAND}" <<'RUBY'
path, command = ARGV
doc = YAML.safe_load_file(path, aliases: false)
abort "FAIL: ci.yml must be a mapping" unless doc.is_a?(Hash)

workflow_env = %w[ASSAY_PUBLIC_MSRV]
job_keys = %w[name needs if runs-on timeout-minutes permissions steps]
step_keys = %w[name uses with run if shell]

abort "FAIL: the workflow has `defaults`; they reach every run step, the counter's included" if doc.key?("defaults")
extra = (doc["env"] || {}).keys - workflow_env
abort "FAIL: the workflow env carries #{extra.join(', ')}; a workflow env reaches the counter's step, so only #{workflow_env.join(', ')} may be set there" unless extra.empty?

jobs = doc.fetch("jobs")
clippy = jobs["clippy"]
abort "FAIL: no `clippy` job" unless clippy.is_a?(Hash)
extra = clippy.keys - job_keys
abort "FAIL: the `clippy` job carries #{extra.join(', ')}; only #{job_keys.join(', ')} are allowed" unless extra.empty?

steps = clippy.fetch("steps")
steps.each_with_index do |step, index|
  abort "FAIL: step #{index + 1} of the `clippy` job is not a mapping" unless step.is_a?(Hash)
  extra = step.keys - step_keys
  abort "FAIL: step #{index + 1} of the `clippy` job carries #{extra.join(', ')}; only #{step_keys.join(', ')} are allowed" unless extra.empty?
  abort "FAIL: step #{index + 1} of the `clippy` job sets shell `#{step['shell']}`; only bash is allowed" if step.key?("shell") && step["shell"] != "bash"
  %w[GITHUB_PATH GITHUB_ENV].each do |file|
    abort "FAIL: step #{index + 1} of the `clippy` job writes #{file}, which changes what a later step runs" if step["run"].to_s.include?(file)
  end
end

found = steps.select { |step| step["run"].is_a?(String) && step["run"].strip == command }
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
    "the command runs without -I@@has no step whose run is exactly@@step['run'] = command.sub(' -I', '')"
    "the step has if: false@@the counter step carries if@@step['if'] = false"
    "the step has shell: bash@@the counter step carries shell@@step['shell'] = 'bash'"
    "the step has continue-on-error@@carries continue-on-error@@step['continue-on-error'] = true"
    "the step has a working-directory@@carries working-directory@@step['working-directory'] = 'docs'"
    "the step has an env@@carries env@@step['env'] = { 'PATH' => '/tmp' }"
    "the job has continue-on-error@@job carries continue-on-error@@clippy['continue-on-error'] = true"
    "the job has a default shell that swallows failures@@job carries defaults@@clippy['defaults'] = { 'run' => { 'shell' => 'bash {0} || true' } }"
    "the job has an env@@job carries env@@clippy['env'] = { 'PATH' => '/tmp' }"
    "the job has a container@@job carries container@@clippy['container'] = 'ubuntu:latest'"
    "the workflow has a default shell that swallows failures@@the workflow has@@doc['defaults'] = { 'run' => { 'shell' => 'bash {0} || true' } }"
    "the workflow env sets PATH@@the workflow env carries PATH@@doc['env'] = (doc['env'] || {}).merge('PATH' => '/tmp')"
    "an earlier step has continue-on-error@@step 4 of the@@clippy['steps'][3]['continue-on-error'] = true"
    "an earlier step has an env@@step 4 of the@@clippy['steps'][3]['env'] = { 'BASH_ENV' => '/tmp/x' }"
    "an earlier step uses another shell@@sets shell@@clippy['steps'][2]['shell'] = 'bash {0} || true'"
    "an earlier step puts a directory first on the path@@writes GITHUB_PATH@@clippy['steps'].unshift({ 'run' => 'echo /tmp/bin >> \"\$GITHUB_PATH\"' })"
    "an earlier step exports a variable to later steps@@writes GITHUB_ENV@@clippy['steps'].unshift({ 'run' => 'echo BASH_ENV=/tmp/x >> \"\$GITHUB_ENV\"' })"
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
  echo "closed-reader-count contract self-test: ${#cases[@]} changed workflows refused, each for its own reason"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
else
  validate "${WORKFLOW}"
fi
