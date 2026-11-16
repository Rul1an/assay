#!/usr/bin/env bash
# Contract for where the reader comparison runs (ADR-052, "What stops the work").
#
# The comparison is a test in crates/assay-cli/tests/docs_corpus_loads.rs. It gives one corpus
# to the last release and to this tree, and without a released binary it says it was skipped.
# It only stops a pull request where a job the required `CI` aggregate waits on installs the
# release, marks the comparison as required, and then runs that test.
#
# This pins that, in the `test` job of ci.yml:
#
#   - one step runs exactly `bash scripts/ci/install-released-assay.sh`, on Linux, with no key
#     that lets it fail quietly;
#   - the step right after it runs exactly the workspace test command, on Linux, with no `env`
#     of its own, so nothing sits between the two that could take the two variables away;
#   - neither the workflow nor the job sets a variable the install script or the test reads,
#     and neither has `defaults` or `continue-on-error`;
#   - the `ci` aggregate needs `test`.
#
# The test itself refuses a released binary that is not the pinned version, and a value of
# ASSAY_READER_COMPARISON that is not `required`; those are pinned there, not here.
#
# Not pinned: the other steps of the `test` job, which is large and changes often. A step
# before these two that rewrites the script, the pin or the test is a change in the diff a
# reviewer reads.
#
# The workflow is parsed, not searched. `--self-test` runs the same check against copies of the
# workflow, each changed in one way the check is meant to refuse, and fails unless every copy is
# refused for that reason.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
INSTALL="bash scripts/ci/install-released-assay.sh"
TESTS="cargo test --locked --workspace --exclude assay-ebpf --exclude assay-it"

validate() {
  ruby -ryaml - "$1" "${INSTALL}" "${TESTS}" <<'RUBY'
path, install, tests = ARGV
doc = YAML.safe_load_file(path, aliases: false)
abort "FAIL: ci.yml must be a mapping" unless doc.is_a?(Hash)

linux = "runner.os == 'Linux'"
reads = /\AASSAY_(RELEASE|READER)/

abort "FAIL: the workflow has `defaults`; they reach every run step" if doc.key?("defaults")
set = (doc["env"] || {}).keys.grep(reads)
abort "FAIL: the workflow env sets #{set.join(', ')}, which the comparison reads" unless set.empty?

jobs = doc.fetch("jobs")
job = jobs["test"]
abort "FAIL: no `test` job" unless job.is_a?(Hash)
%w[defaults continue-on-error].each do |key|
  abort "FAIL: the `test` job has #{key}" if job.key?(key)
end
set = (job["env"] || {}).keys.grep(reads)
abort "FAIL: the `test` job env sets #{set.join(', ')}, which the comparison reads" unless set.empty?

steps = job.fetch("steps")
where = lambda do |command|
  found = steps.each_index.select { |i| steps[i].is_a?(Hash) && steps[i]["run"].is_a?(String) && steps[i]["run"].strip == command }
  abort "FAIL: the `test` job has no step whose run is exactly `#{command}`" if found.empty?
  abort "FAIL: the `test` job runs `#{command}` #{found.size} times" if found.size > 1
  found.first
end
at_install = where.call(install)
at_tests = where.call(tests)

{ at_install => ["the install step", %w[name if shell run]], at_tests => ["the test step", %w[name if run]] }.each do |index, (what, allowed)|
  step = steps[index]
  extra = step.keys - allowed
  abort "FAIL: #{what} carries #{extra.join(', ')}; only #{allowed.join(', ')} are allowed" unless extra.empty?
  abort "FAIL: #{what} runs if `#{step['if']}`; it must be `#{linux}`" unless step["if"] == linux
  abort "FAIL: #{what} sets shell `#{step['shell']}`; only bash is allowed" if step.key?("shell") && step["shell"] != "bash"
end
abort "FAIL: the test step must come right after the install step; it is #{at_tests - at_install} step(s) after it" unless at_tests == at_install + 1

needs = Array(jobs.fetch("ci").fetch("needs"))
abort "FAIL: the `ci` aggregate does not need `test`" unless needs.include?("test")
puts "reader-comparison contract: the test job installs the last release and then runs the workspace tests, and the ci aggregate needs it"
RUBY
}

self_test() {
  local tmp failed=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  # name @@ expected message @@ ruby that edits the parsed workflow
  local cases=(
    "the install step is removed@@has no step whose run is exactly \`bash@@steps.delete_at(at_install)"
    "the install command is followed by || true@@has no step whose run is exactly \`bash@@steps[at_install]['run'] = install + ' || true'"
    "the install step has if: false@@the install step runs if@@steps[at_install]['if'] = false"
    "the install step runs on Windows only@@the install step runs if@@steps[at_install]['if'] = \"runner.os == 'Windows'\""
    "the install step has no if@@the install step runs if@@steps[at_install].delete('if')"
    "the install step has continue-on-error@@the install step carries continue-on-error@@steps[at_install]['continue-on-error'] = true"
    "the install step has an env@@the install step carries env@@steps[at_install]['env'] = { 'ASSAY_RELEASED_INSTALLER' => '/bin/true' }"
    "the install step uses a shell that swallows failures@@the install step sets shell@@steps[at_install]['shell'] = 'bash {0} || true'"
    "the install step runs twice@@2 times@@steps.insert(at_install, steps[at_install].dup)"
    "the install step comes after the tests@@right after the install step@@steps.insert(at_tests + 1, steps.delete_at(at_install))"
    "a step sits between the two@@right after the install step@@steps.insert(at_tests, { 'run' => 'echo ASSAY_READER_COMPARISON= >> \"\$GITHUB_ENV\"' })"
    "the test command is followed by || true@@has no step whose run is exactly \`cargo@@steps[at_tests]['run'] = tests + ' || true'"
    "the test command skips the comparison@@has no step whose run is exactly \`cargo@@steps[at_tests]['run'] = tests + ' -- --skip the_last_release'"
    "the test step has an env that takes the variable away@@the test step carries env@@steps[at_tests]['env'] = { 'ASSAY_READER_COMPARISON' => '' }"
    "the test step has continue-on-error@@the test step carries continue-on-error@@steps[at_tests]['continue-on-error'] = true"
    "the test step has if: false@@the test step runs if@@steps[at_tests]['if'] = false"
    "the job env sets the variable@@job env sets ASSAY_READER_COMPARISON@@job['env'] = (job['env'] || {}).merge('ASSAY_READER_COMPARISON' => 'no')"
    "the job env replaces the installer@@job env sets ASSAY_RELEASED_INSTALLER@@job['env'] = (job['env'] || {}).merge('ASSAY_RELEASED_INSTALLER' => '/bin/true')"
    "the workflow env replaces the pin@@workflow env sets ASSAY_RELEASE_TAG_FILE@@doc['env'] = (doc['env'] || {}).merge('ASSAY_RELEASE_TAG_FILE' => '/tmp/pin')"
    "the job has continue-on-error@@job has continue-on-error@@job['continue-on-error'] = true"
    "the job has a default shell that swallows failures@@job has defaults@@job['defaults'] = { 'run' => { 'shell' => 'bash {0} || true' } }"
    "the workflow has a default shell that swallows failures@@the workflow has@@doc['defaults'] = { 'run' => { 'shell' => 'bash {0} || true' } }"
    "the aggregate does not need test@@aggregate does not need@@doc['jobs']['ci']['needs'] = Array(doc['jobs']['ci']['needs']) - ['test']"
  )

  local entry name rest expected edit out code
  for entry in "${cases[@]}"; do
    name="${entry%%@@*}"
    rest="${entry#*@@}"
    expected="${rest%%@@*}"
    edit="${rest#*@@}"
    ruby -ryaml - "${WORKFLOW}" "${INSTALL}" "${TESTS}" "${tmp}/ci.yml" "${edit}" <<'RUBY'
path, install, tests, out, edit = ARGV
doc = YAML.safe_load_file(path, aliases: false)
job = doc['jobs']['test']
steps = job['steps']
at = ->(command) { steps.index { |s| s.is_a?(Hash) && s['run'].to_s.strip == command } }
at_install = at.call(install)
at_tests = at.call(tests)
abort "self-test: the real workflow lacks the install step or the test step" if at_install.nil? || at_tests.nil?
eval(edit)
File.write(out, YAML.dump(doc))
RUBY
    set +e
    out="$(validate "${tmp}/ci.yml" 2>&1)"
    code=$?
    set -e
    if [[ ${code} -eq 0 ]]; then
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
  echo "reader-comparison contract self-test: ${#cases[@]} changed workflows refused, each for its own reason"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
else
  validate "${WORKFLOW}"
fi
