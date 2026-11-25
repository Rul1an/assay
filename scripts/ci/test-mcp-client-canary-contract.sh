#!/usr/bin/env bash
# Contract for the MCP client canary wiring (.github/workflows/mcp-client-canary.yml).
#
# The canary is only worth its run if the run can fail: the canary step must execute the runner
# exactly, unconditionally and without a swallowed exit code, against packages installed from the
# lockfile and an assay built from the locked tree; and Dependabot must keep moving the client.
# Each claim is read from the parsed documents, never from text, and every rule has a mutation
# below that must make it fail.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/mcp-client-canary.yml"
DEPENDABOT="${ROOT}/.github/dependabot.yml"
FIXTURE="${ROOT}/scripts/ci/mcp-client-canary"

validate() {
  ruby -ryaml -rjson - "$1" "$2" "$3" <<'RUBY'
workflow_path, dependabot_path, fixture = ARGV
def fail!(message) = abort("FAIL: #{message}")
wf = Psych.safe_load_file(workflow_path, aliases: false)
jobs = wf.fetch('jobs')
fail!('expected exactly one job named canary') unless jobs.keys == ['canary']
job = jobs['canary']
fail!('canary job must run on ubuntu-24.04') unless job['runs-on'] == 'ubuntu-24.04'
fail!('canary job needs a timeout of at most 60 minutes') unless job['timeout-minutes'].is_a?(Integer) && job['timeout-minutes'].between?(1, 60)
%w[if continue-on-error].each { |key| fail!("canary job must not set #{key}") if job.key?(key) }
unless job['permissions'] == { 'contents' => 'read', 'attestations' => 'read' }
  fail!("canary job permissions must be exactly contents: read, attestations: read, got #{job['permissions']}")
end
steps = job.fetch('steps')
steps.each do |step|
  uses = step['uses']
  next if uses.nil? || uses.start_with?('./.github/actions/')
  fail!("action not pinned to a full SHA: #{uses}") unless uses.match?(/\A[^@\s]+@[0-9a-f]{40}\z/)
end
def exactly_one(steps, run)
  found = steps.select { |s| s['run'].is_a?(String) && s['run'].strip == run }
  fail!("expected exactly one step running `#{run}`, found #{found.length}") unless found.length == 1
  step = found.first
  %w[if continue-on-error].each { |key| fail!("step `#{run}` must not set #{key}") if step.key?(key) }
  step
end
install = exactly_one(steps, 'npm ci --ignore-scripts --no-audit --no-fund')
fail!('npm ci must run in scripts/ci/mcp-client-canary') unless install['working-directory'] == 'scripts/ci/mcp-client-canary'
build = exactly_one(steps, 'cargo build --locked -p assay-cli --bin assay')
canary = exactly_one(steps, 'node scripts/ci/mcp-client-canary/canary.mjs --assay head=target/debug/assay --assay "release=${RUNNER_TEMP}/assay-release/assay"')
fail!('the canary must be the last step') unless steps.last.equal?(canary)
# The release leg installs exactly what the recipe names, through the user installer, with provenance.
releases = steps.select { |s| s['name'] == 'Install the release the recipe names' }
fail!("expected exactly one release install step, found #{releases.length}") unless releases.length == 1
release = releases.first
%w[if continue-on-error].each { |key| fail!("release install step must not set #{key}") if release.key?(key) }
fail!('release install needs GH_TOKEN from github.token') unless release['env'] == { 'GH_TOKEN' => '${{ github.token }}' }
lines = release['run'].to_s.lines.map(&:strip).reject(&:empty?)
expected = [
  'set -euo pipefail',
  'version="$(node scripts/ci/mcp-client-canary/canary.mjs --recipe-release)"',
  'echo "recipe names Assay ${version}; install pin $(cat .github/assay-release-tag)"',
  'ASSAY_VERSION="${version}" ASSAY_REQUIRE_PROVENANCE=1 ASSAY_INSTALL_DIR="${RUNNER_TEMP}/assay-release" sh scripts/install.sh',
]
fail!("release install step must run exactly #{expected}, got #{lines}") unless lines == expected
order = [steps.index { |s| s.equal?(build) }, steps.index { |s| s.equal?(release) }, steps.index { |s| s.equal?(canary) }]
fail!('build, release install and canary must run in that order') unless order == order.sort && order.uniq.length == 3
on = wf['on'] || wf[true]
fail!('the canary needs a schedule') unless on.is_a?(Hash) && on['schedule'].is_a?(Array) && !on['schedule'].empty?
pr_paths = on.dig('pull_request', 'paths') || []
['scripts/ci/mcp-client-canary/**', 'crates/assay-core/src/mcp/**', 'docs/guides/editor-mcp-recipe.md'].each do |p|
  fail!("pull_request paths must include #{p}") unless pr_paths.include?(p)
end
updates = Psych.safe_load_file(dependabot_path, aliases: false).fetch('updates')
npm = updates.select { |u| u['package-ecosystem'] == 'npm' && u['directory'] == '/scripts/ci/mcp-client-canary' }
fail!('expected one npm Dependabot entry for /scripts/ci/mcp-client-canary') unless npm.length == 1
allowed = (npm.first['allow'] || []).map { |a| a['dependency-name'] }
fail!("Dependabot must move only @modelcontextprotocol/client, allows #{allowed}") unless allowed == ['@modelcontextprotocol/client']
manifest = JSON.parse(File.read(File.join(fixture, 'package.json')))
manifest.fetch('dependencies').each do |name, version|
  fail!("#{name} must be pinned exactly, got #{version}") unless version.match?(/\A[0-9]+\.[0-9]+\.[0-9]+\z/)
end
lock = JSON.parse(File.read(File.join(fixture, 'package-lock.json')))
fail!('package-lock.json must be lockfileVersion 3') unless lock['lockfileVersion'] == 3
manifest['dependencies'].each do |name, version|
  locked = lock.dig('packages', "node_modules/#{name}", 'version')
  fail!("lock has #{name} #{locked}, package.json pins #{version}") unless locked == version
end
RUBY
}

validate "${WORKFLOW}" "${DEPENDABOT}" "${FIXTURE}"

# Must-bite controls: every mutation must make the contract fail.
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT
canary_run='node scripts/ci/mcp-client-canary/canary.mjs --assay head=target/debug/assay --assay "release=${RUNNER_TEMP}/assay-release/assay"'
installer_run='ASSAY_VERSION="${version}" ASSAY_REQUIRE_PROVENANCE=1 ASSAY_INSTALL_DIR="${RUNNER_TEMP}/assay-release" sh scripts/install.sh'
mutate() {
  local name="$1" from="$2" to="$3" file="${4:-${WORKFLOW}}"
  local out="${tmp}/${name}"
  python3 - "${file}" "${out}" "${from}" "${to}" <<'PY'
import sys
source, out, old, new = sys.argv[1:]
text = open(source).read()
assert text.count(old) == 1, (old, text.count(old))
open(out, 'w').write(text.replace(old, new))
PY
  printf '%s' "${out}"
}
expect_fail() {
  local label="$1" workflow="$2" dependabot="${3:-${DEPENDABOT}}"
  if validate "${workflow}" "${dependabot}" "${FIXTURE}" 2>/dev/null; then
    echo "FAIL: mutation '${label}' was not caught" >&2
    exit 1
  fi
}
expect_fail 'exit code swallowed' "$(mutate swallow "run: ${canary_run}" "run: ${canary_run} || true")"
expect_fail 'step commented out' "$(mutate comment "run: ${canary_run}" "run: echo skipped # ${canary_run}")"
expect_fail 'step if false' "$(mutate step-if "      - name: Run the canary
" "      - name: Run the canary
        if: false
")"
expect_fail 'step continue-on-error' "$(mutate step-coe "      - name: Run the canary
" "      - name: Run the canary
        continue-on-error: true
")"
expect_fail 'job if false' "$(mutate job-if "    timeout-minutes: 45
" "    timeout-minutes: 45
    if: false
")"
expect_fail 'npm install instead of ci' "$(mutate npm-install 'run: npm ci --ignore-scripts --no-audit --no-fund' 'run: npm install --ignore-scripts --no-audit --no-fund')"
expect_fail 'unlocked build' "$(mutate unlocked 'cargo build --locked -p assay-cli' 'cargo build -p assay-cli')"
expect_fail 'floating action' "$(mutate floating 'actions/setup-node@48b55a011bda9f5d6aeb4c2d9c7362e8dae4041e' 'actions/setup-node@v6')"
expect_fail 'no schedule' "$(mutate no-schedule '  schedule:
    - cron: "23 5 * * *"
' '')"
expect_fail 'Dependabot moves every package' "${WORKFLOW}" "$(mutate allow-all '      - dependency-name: "@modelcontextprotocol/client"' '      - dependency-name: "*"' "${DEPENDABOT}")"
expect_fail 'release leg dropped' "$(mutate no-release " --assay \"release=\${RUNNER_TEMP}/assay-release/assay\"" '')"
expect_fail 'provenance not required' "$(mutate no-provenance ' ASSAY_REQUIRE_PROVENANCE=1' '')"
expect_fail 'installer failure swallowed' "$(mutate installer-true "${installer_run}" "${installer_run} || true")"
expect_fail 'release version not from the recipe' "$(mutate fixed-version 'version="$(node scripts/ci/mcp-client-canary/canary.mjs --recipe-release)"' 'version="8.0.1"')"
expect_fail 'attestations permission dropped' "$(mutate no-attestations '      attestations: read
' '')"
echo "mcp-client-canary contract: ok (15 mutations caught)"
