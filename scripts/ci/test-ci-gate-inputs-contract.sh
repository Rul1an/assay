#!/usr/bin/env bash
# Contract for what the required `CI` aggregate decides from.
#
# The `ci` job of ci.yml ends in the step "Evaluate required job results". Its `run` turns a set
# of variables into the verdict of the one required context, and its `env` says where each
# variable comes from. A variable that is a literal, or that reads another job, leaves the
# verdict green whatever the job it is named after did. The same holds one step earlier: the
# `scope` job publishes the outputs that let jobs be skipped, and an output that is a literal
# skips them on every run.
#
# What can stand in an `env` value is not a list that can be finished by naming cases, so the
# rule is a closed shape instead.
#
# In the `scope` job:
#
#   - `outputs` is a mapping, and every output `<name>` is exactly
#     `${{ steps.detect.outputs.<name> }}`: the output of that name of the one step with id
#     `detect`, the step that classifies the change. The rule is the same for every output, so
#     an output added later is covered without this file changing.
#
# In the `ci` job:
#
#   - one step in the whole workflow is named "Evaluate required job results", and it is in
#     `ci`. It has `name`, `env`, `shell` and `run` and nothing else, so no `if` and no
#     `continue-on-error`, and its shell is bash;
#   - for every job in `needs`, that step's `env` sets `<JOB>_RESULT` (the job id in capitals,
#     `-` written as `_`) to exactly `${{ needs.<job>.result }}`. The names are derived from
#     `needs`, so a job added there needs its variable;
#   - the other inputs of the step are the ones listed in `others` below, each with exactly the
#     expression written there: three scope outputs, two outputs of the semver workflow, and the
#     override reason and actor of a manual run;
#   - the step's `env` holds nothing else. A step's `env` wins over the job's and the
#     workflow's, so with every variable set there, neither can replace one;
#   - every scope output the step reads is one the `scope` job declares, and the aggregate
#     needs both jobs whose outputs it reads. That ties the chain end to end: the `detect` step,
#     the output, the variable.
#
# An expression is compared as it is written. `${{needs.test.result}}` means the same to GitHub
# and is refused here, like every other spelling: only one text is allowed.
#
# `scripts/ci/check-ci-gate-coverage.py` reads the same `env` by line, across the whole job,
# because it has to read comments a YAML parser drops. It refuses a result variable that is a
# literal, and it passes when the real expression stands anywhere else in the job, in another
# step's `env` or as text in a `run`. This file is the reading of the parsed step. Both must
# hold; neither replaces the other.
#
# Not pinned here:
#
#   - the `run` of the gate step. How the variables become a verdict is run as it stands by
#     `scripts/ci/test-ci-gate-expectations.sh`, and its table of jobs is read by
#     `scripts/ci/check-ci-gate-coverage.py`. A body that sets a variable again, or stops
#     reading one, is theirs to refuse;
#   - what the `detect` step writes. `scripts/ci/test-classify-lightweight-changes.sh` pins how
#     `lightweight_only` is computed; this file only says the outputs come from that step. The
#     step writing a literal for another output, such as `ebpf_smoke_required=false`, is not
#     refused here;
#   - the `if` of each job that reads a scope output. A job that is skipped while the gate
#     expected it turns the gate red when it runs;
#   - which jobs are in `needs`, and the `ci` job's own `if`. The coverage check pins the
#     first; the timeout and call-site contracts refuse a changed `if`;
#   - an earlier step of the `ci` job that writes GITHUB_ENV, for example a BASH_ENV, and so
#     changes what the gate step's shell does before the body starts. That is a change to a
#     `run` text, in the diff a reviewer reads;
#   - a line-based reader that finds the gate step by its name inside the text of a `run`;
#   - `.github/workflows/semver-public.yml`, where the two semver outputs are produced.
#
# The workflow is parsed, not searched. `--self-test` runs the same check against copies of the
# workflow, each changed in one way the check is meant to refuse, and fails unless every copy is
# refused for that reason.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
GATE_STEP="Evaluate required job results"
CLASSIFIER="detect"

validate() {
  ruby -ryaml - "$1" "${GATE_STEP}" "${CLASSIFIER}" <<'RUBY'
path, gate_name, classifier = ARGV
doc = YAML.safe_load_file(path, aliases: false)
abort "FAIL: ci.yml must be a mapping" unless doc.is_a?(Hash)
jobs = doc["jobs"]
abort "FAIL: ci.yml has no jobs" unless jobs.is_a?(Hash)

wrap = ->(inner) { "${{ #{inner} }}" }
show = ->(value) { value.is_a?(String) ? value : value.inspect }
result_of = /\A\$\{\{ needs\.([A-Za-z0-9_-]+)\.result \}\}\z/
output_of = /\A\$\{\{ steps\.([A-Za-z0-9_-]+)\.outputs\.([A-Za-z0-9_-]+) \}\}\z/

# The inputs of the gate step that are not a job result. Each is the whole expression.
others = {
  "LIGHTWEIGHT_ONLY" => "needs.scope.outputs.lightweight_only",
  "EBPF_SMOKE_REQUIRED" => "needs.scope.outputs.ebpf_smoke_required",
  "MCP_REGISTRY_TOUCHED" => "needs.scope.outputs.mcp_registry_touched",
  "AEE_SEAL_MUTATION_TOUCHED" => "needs.scope.outputs.aee_seal_mutation_touched",
  "SEMVER_RELEVANT" => "needs.semver.outputs['semver_relevant']",
  "SEMVER_PUBLIC_RESULT" => "needs.semver.outputs['semver_public_result']",
  "SEMVER_OVERRIDE_REASON" => "github.event_name == 'workflow_dispatch' && inputs.override_reason || ''",
  "SEMVER_OVERRIDE_ACTOR" => "github.event_name == 'workflow_dispatch' && github.actor || ''",
}.transform_values(&wrap)

# --- the scope job: every output is the output of that name of the classifying step.
scope = jobs["scope"]
abort "FAIL: no `scope` job" unless scope.is_a?(Hash)
outputs = scope["outputs"]
abort "FAIL: the `scope` job has no outputs" unless outputs.is_a?(Hash) && !outputs.empty?
with_id = Array(scope["steps"]).select { |s| s.is_a?(Hash) && s["id"] == classifier }
abort "FAIL: the `scope` job has no step with id `#{classifier}`, the step every scope output must come from" if with_id.empty?
abort "FAIL: the `scope` job has #{with_id.size} steps with id `#{classifier}`" if with_id.size > 1
outputs.each do |name, value|
  want = wrap.call("steps.#{classifier}.outputs.#{name}")
  next if value == want
  from = value.is_a?(String) ? output_of.match(value) : nil
  if from && from[1] != classifier
    abort "FAIL: the scope output `#{name}` comes from the step `#{from[1]}`; every scope output must come from the `#{classifier}` step"
  elsif from
    abort "FAIL: the scope output `#{name}` is the `#{from[2]}` output of the `#{classifier}` step; it must be that step's `#{name}` output"
  else
    abort "FAIL: the scope output `#{name}` is `#{show.call(value)}`; it must be `#{want}`, what the `#{classifier}` step computed"
  end
end

# --- the gate step: found by its name, once in the workflow, in the aggregate.
ci = jobs["ci"]
abort "FAIL: no `ci` job" unless ci.is_a?(Hash)
named = ->(job) { Array(job["steps"]).select { |s| s.is_a?(Hash) && s["name"] == gate_name } }
elsewhere = jobs.select { |id, job| id != "ci" && job.is_a?(Hash) && !named.call(job).empty? }.keys
abort "FAIL: the job `#{elsewhere.join('`, `')}` has a step named `#{gate_name}`; the gate step is found by that name, so only the `ci` aggregate may have one" unless elsewhere.empty?
found = named.call(ci)
abort "FAIL: the `ci` aggregate has no step named `#{gate_name}`" if found.empty?
abort "FAIL: the `ci` aggregate has #{found.size} steps named `#{gate_name}`" if found.size > 1
step = found.first
allowed = %w[name env shell run]
extra = step.keys - allowed
abort "FAIL: the gate step carries #{extra.join(', ')}; only #{allowed.join(', ')} are allowed" unless extra.empty?
abort "FAIL: the gate step sets shell `#{step['shell']}`; only bash is allowed" unless step["shell"] == "bash"
abort "FAIL: the gate step has no run" unless step["run"].is_a?(String)
env = step["env"]
abort "FAIL: the gate step has no env; every variable its run reads must be set there" unless env.is_a?(Hash)

# --- its env: one result variable per needed job, the inputs in `others`, and nothing else.
needs = Array(ci["needs"])
abort "FAIL: the `ci` aggregate needs no job" if needs.empty?
variable = ->(job) { job.to_s.upcase.tr("-", "_") + "_RESULT" }
results = needs.to_h { |job| [variable.call(job), job] }
abort "FAIL: two jobs in the aggregate's `needs` share one result variable" unless results.size == needs.size
clash = results.keys & others.keys
abort "FAIL: #{clash.join(', ')} would be both a job result and another input of the gate step" unless clash.empty?

results.each do |name, job|
  abort "FAIL: the aggregate needs `#{job}`, but the gate step sets no `#{name}`" unless env.key?(name)
  value = env[name]
  next if value == wrap.call("needs.#{job}.result")
  other = value.is_a?(String) ? result_of.match(value) : nil
  if other
    abort "FAIL: `#{name}` reads the result of `#{other[1]}`; it must read the result of `#{job}`"
  else
    abort "FAIL: `#{name}` is `#{show.call(value)}`; it must be `#{wrap.call("needs.#{job}.result")}`, the result of the job it is named after"
  end
end
others.each do |name, want|
  abort "FAIL: the gate step sets no `#{name}`; it must be `#{want}`, set in the step so no job or workflow env can supply it" unless env.key?(name)
  abort "FAIL: `#{name}` is `#{show.call(env[name])}`; it must be `#{want}`" unless env[name] == want
end
(env.keys - results.keys - others.keys).each do |name|
  other = env[name].is_a?(String) ? result_of.match(env[name]) : nil
  if other && needs.include?(other[1])
    abort "FAIL: the gate step carries `#{name}`, the result of `#{other[1]}` under a second name"
  elsif other
    abort "FAIL: the gate step carries `#{name}`, the result of `#{other[1]}`, a job the aggregate does not need"
  else
    abort "FAIL: the gate step carries `#{name}`; only a result variable per needed job and #{others.keys.join(', ')} may be set there"
  end
end

# --- the chain: what the gate step reads is declared, and comes from a job the aggregate needs.
others.each_value do |want|
  want.scan(/needs\.scope\.outputs\.([A-Za-z0-9_]+)/).flatten.each do |name|
    abort "FAIL: the gate step reads the scope output `#{name}`, which the `scope` job does not declare" unless outputs.key?(name)
  end
end
others.values.join.scan(/needs\.([A-Za-z0-9_-]+)\.outputs/).flatten.uniq.each do |job|
  abort "FAIL: the gate step reads outputs of `#{job}`, but the aggregate does not need `#{job}`" unless needs.include?(job)
end

puts "ci-gate-inputs contract: the gate step reads #{results.size} job results and #{others.size} other inputs, each from what it is named after, and #{outputs.size} scope outputs come from the `#{classifier}` step"
RUBY
}

self_test() {
  local tmp failed=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  # name @@ expected message @@ ruby that edits the parsed workflow
  # In the edits, res.('job') is the result expression of a job and x.('...') wraps an expression.
  local cases=(
    "a result variable is a literal@@\`TEST_RESULT\` is \`success\`@@env['TEST_RESULT'] = 'success'"
    "a result variable reads another job@@\`TEST_RESULT\` reads the result of \`clippy\`@@env['TEST_RESULT'] = res.('clippy')"
    "two result variables are swapped@@\`CLIPPY_RESULT\` reads the result of \`rustdoc\`@@env['CLIPPY_RESULT'], env['RUSTDOC_RESULT'] = env['RUSTDOC_RESULT'], env['CLIPPY_RESULT']"
    "a result variable is removed@@needs \`test\`, but the gate step sets no \`TEST_RESULT\`@@env.delete('TEST_RESULT')"
    "a needed job has no result variable@@needs \`lint\`, but the gate step sets no \`LINT_RESULT\`@@ci['needs'] << 'lint'"
    "a result variable falls back to success@@\`TEST_RESULT\` is \`\${{ needs.test.result || 'success' }}\`@@env['TEST_RESULT'] = x.(\"needs.test.result || 'success'\")"
    "a result is read in another spelling@@\`TEST_RESULT\` is \`\${{needs.test.result}}\`@@env['TEST_RESULT'] = '\${{needs.test.result}}'"
    "the real expression moves to another step and the gate step reads a literal@@\`TEST_RESULT\` is \`success\`@@steps.insert(at, { 'name' => 'Record the test result', 'env' => { 'TEST_RESULT' => res.('test') }, 'run' => 'true' }); env['TEST_RESULT'] = 'success'"
    "the real expression stays as text in another run and the gate step reads a literal@@\`TEST_RESULT\` is \`success\`@@steps.insert(at, { 'name' => 'Note the binding', 'run' => \": <<'NOTE'\\nTEST_RESULT: \" + res.('test') + \"\\nNOTE\\n\" }); env['TEST_RESULT'] = 'success'"
    "a result is carried under a second name@@carries \`SHADOW_RESULT\`, the result of \`test\` under a second name@@env['SHADOW_RESULT'] = res.('test')"
    "the aggregate stops needing a job and keeps its variable@@carries \`PERF_RESULT\`, the result of \`perf\`, a job the aggregate does not need@@ci['needs'].delete('perf')"
    "the gate step sets a variable this contract does not name@@carries \`FORCE_GREEN\`; only@@env['FORCE_GREEN'] = '1'"
    "the gate step reads a literal for lightweight_only@@\`LIGHTWEIGHT_ONLY\` is \`true\`@@env['LIGHTWEIGHT_ONLY'] = 'true'"
    "the gate step reads a literal for ebpf_smoke_required@@\`EBPF_SMOKE_REQUIRED\` is \`false\`@@env['EBPF_SMOKE_REQUIRED'] = 'false'"
    "the gate step reads a literal for mcp_registry_touched@@\`MCP_REGISTRY_TOUCHED\` is \`false\`@@env['MCP_REGISTRY_TOUCHED'] = 'false'"
    "the gate step reads a literal for semver_relevant@@\`SEMVER_RELEVANT\` is \`false\`@@env['SEMVER_RELEVANT'] = 'false'"
    "the gate step reads a literal for the inner semver result@@\`SEMVER_PUBLIC_RESULT\` is \`success\`@@env['SEMVER_PUBLIC_RESULT'] = 'success'"
    "the semver override reason is set on every run@@\`SEMVER_OVERRIDE_REASON\` is \`waived\`@@env['SEMVER_OVERRIDE_REASON'] = 'waived'"
    "the gate step reads another scope output as lightweight_only@@\`LIGHTWEIGHT_ONLY\` is \`\${{ needs.scope.outputs.docs_generated_drift_inputs_touched }}\`@@env['LIGHTWEIGHT_ONLY'] = x.('needs.scope.outputs.docs_generated_drift_inputs_touched')"
    "the gate step sets no lightweight_only, so a job env could@@the gate step sets no \`LIGHTWEIGHT_ONLY\`@@env.delete('LIGHTWEIGHT_ONLY')"
    "the gate step has if: false@@the gate step carries if@@step['if'] = false"
    "the gate step has continue-on-error@@the gate step carries continue-on-error@@step['continue-on-error'] = true"
    "the gate step inherits its shell@@the gate step sets shell@@step.delete('shell')"
    "the gate step uses a shell that swallows failures@@the gate step sets shell@@step['shell'] = 'bash {0} || true'"
    "the gate step has no env@@the gate step has no env@@step.delete('env')"
    "the gate step is renamed@@aggregate has no step named@@step['name'] = 'Evaluate results'"
    "a second step of the aggregate has the gate step's name@@aggregate has 2 steps named@@steps.insert(at, { 'name' => step['name'], 'run' => 'true' })"
    "a step of another job has the gate step's name@@the job \`clippy\` has a step named@@jobs['clippy']['steps'] << { 'name' => step['name'], 'run' => 'true' }"
    "the aggregate stops needing scope and drops its variable@@the aggregate does not need \`scope\`@@ci['needs'].delete('scope'); env.delete('SCOPE_RESULT')"
    "a scope output is a literal@@the scope output \`lightweight_only\` is \`true\`@@outputs['lightweight_only'] = 'true'"
    "the ebpf scope output is a literal@@the scope output \`ebpf_smoke_required\` is \`false\`@@outputs['ebpf_smoke_required'] = 'false'"
    "the registry scope output is a literal@@the scope output \`mcp_registry_touched\` is \`false\`@@outputs['mcp_registry_touched'] = 'false'"
    "a scope output comes from another step@@the scope output \`lightweight_only\` comes from the step \`always\`@@scope['steps'].unshift({ 'id' => 'always', 'run' => 'echo lightweight_only=true >> \"\$GITHUB_OUTPUT\"' }); outputs['lightweight_only'] = x.('steps.always.outputs.lightweight_only')"
    "a scope output is another output of the classifying step@@the scope output \`ebpf_smoke_required\` is the \`mcp_registry_touched\` output@@outputs['ebpf_smoke_required'] = x.('steps.detect.outputs.mcp_registry_touched')"
    "a scope output falls back to a literal@@the scope output \`lightweight_only\` is \`\${{ steps.detect.outputs.lightweight_only || 'true' }}\`@@outputs['lightweight_only'] = x.(\"steps.detect.outputs.lightweight_only || 'true'\")"
    "a new scope output is a literal@@the scope output \`docs_only\` is \`true\`@@outputs['docs_only'] = 'true'"
    "the classifying step loses its id@@has no step with id \`detect\`@@detect.delete('id')"
    "two steps carry the classifying step's id@@has 2 steps with id \`detect\`@@scope['steps'].unshift({ 'id' => 'detect', 'run' => 'true' })"
    "the scope job has no outputs@@the \`scope\` job has no outputs@@scope.delete('outputs')"
    "a scope output the gate step reads is not declared@@reads the scope output \`mcp_registry_touched\`, which the \`scope\` job does not declare@@outputs.delete('mcp_registry_touched')"
  )

  local entry name rest expected edit out code
  for entry in "${cases[@]}"; do
    name="${entry%%@@*}"
    rest="${entry#*@@}"
    expected="${rest%%@@*}"
    edit="${rest#*@@}"
    ruby -ryaml - "${WORKFLOW}" "${GATE_STEP}" "${CLASSIFIER}" "${tmp}/ci.yml" "${edit}" <<'RUBY'
path, gate_name, classifier, out, edit = ARGV
doc = YAML.safe_load_file(path, aliases: false)
jobs = doc['jobs']
scope = jobs['scope']
outputs = scope['outputs']
detect = scope['steps'].find { |s| s.is_a?(Hash) && s['id'] == classifier }
ci = jobs['ci']
steps = ci['steps']
at = steps.index { |s| s.is_a?(Hash) && s['name'] == gate_name }
abort "self-test: the real workflow lacks the gate step, its env, the scope outputs or the classifying step" if at.nil? || detect.nil? || !outputs.is_a?(Hash) || !steps[at]['env'].is_a?(Hash)
step = steps[at]
env = step['env']
x = ->(inner) { "${{ #{inner} }}" }
res = ->(job) { x.call("needs.#{job}.result") }
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
  echo "ci-gate-inputs contract self-test: ${#cases[@]} changed workflows refused, each for its own reason"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
else
  validate "${WORKFLOW}"
fi
