#!/usr/bin/env bash
# Contract: the required CI workflow cancels a pull request run once a newer head of the same pull
# request exists, and never cancels or drops any other run.
#
# Branch protection judges only the newest head of a pull request, so a run on an older head holds
# runners for a result nobody reads: over 31 days to 2026-11-25, 71 of 494 pull request runs of
# ci.yml kept running after a newer head existed, 902 wall-clock minutes in all. One push starts
# about 50 jobs, so those runs delayed every other pull request and main.
#
# Two properties, both read from the parsed workflow:
#   1. Pull request runs share one group per pull request number and cancel in progress.
#   2. Every other run (push to main, debug/**, docs/auto-update; workflow_dispatch) gets a group
#      of its own (github.run_id) and is never cancelled. A shared group would not be safe even
#      with cancel-in-progress false: GitHub cancels a run that is still pending in a group when a
#      newer one queues behind it, so main runs would be dropped silently.
# The expressions are pinned exactly; changing either is a decision this test makes visible.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${ROOT}/.github/workflows/ci.yml"
[[ -f "$WORKFLOW" ]] || { echo "FAIL: missing ${WORKFLOW#"${ROOT}"/}" >&2; exit 1; }

ruby -ryaml - "$WORKFLOW" <<'RUBY'
path = ARGV.fetch(0)
doc = YAML.safe_load_file(path, aliases: false)

GROUP = "ci-${{ github.event_name == 'pull_request' && format('pr-{0}', github.event.pull_request.number) || format('run-{0}', github.run_id) }}"
CANCEL = "${{ github.event_name == 'pull_request' }}"

def problems(doc)
  out = []
  return ["ci.yml is not a mapping"] unless doc.is_a?(Hash)
  conc = doc["concurrency"]
  return ["ci.yml has no workflow-level concurrency"] unless conc.is_a?(Hash)
  keys = conc.keys.sort
  out << "concurrency keys must be cancel-in-progress and group; got #{keys.join(', ')}" unless keys == %w[cancel-in-progress group]
  out << "concurrency group must be exactly #{GROUP}" unless conc["group"] == GROUP
  out << "cancel-in-progress must be exactly #{CANCEL}" unless conc["cancel-in-progress"] == CANCEL
  out
end

real = problems(doc)
unless real.empty?
  warn real.map { |p| "FAIL: #{p}" }.join("\n")
  exit 1
end

def mutated(doc)
  copy = Marshal.load(Marshal.dump(doc))
  yield copy
  copy
end

mutations = {
  "concurrency removed" => lambda { |d| d.delete("concurrency") },
  "every run cancels" => lambda { |d| d["concurrency"]["cancel-in-progress"] = true },
  "nothing cancels" => lambda { |d| d["concurrency"]["cancel-in-progress"] = false },
  "one group per ref" => lambda { |d| d["concurrency"]["group"] = "ci-${{ github.ref }}" },
  "one group for the workflow" => lambda { |d| d["concurrency"]["group"] = "ci" },
  "pushes share a group by sha" => lambda { |d| d["concurrency"]["group"] = d["concurrency"]["group"].sub("github.run_id", "github.sha") },
  "extra key" => lambda { |d| d["concurrency"]["queue"] = "max" },
}
survived = mutations.select { |_, m| problems(mutated(doc, &m)).empty? }.keys
unless survived.empty?
  warn "FAIL: the contract did not refuse: #{survived.join(', ')}"
  exit 1
end
puts "ci concurrency contract: pull request runs cancel per PR, other runs never; #{mutations.length} mutations refused"
RUBY
