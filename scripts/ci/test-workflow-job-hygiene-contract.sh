#!/usr/bin/env bash
# Contract: two hygiene rules for every job in every workflow under .github/workflows.
#
# 1. Every job states its own timeout-minutes: an integer from 1 to 120, or a ${{ }} expression
#    (a matrix-dependent ceiling). A job without one inherits GitHub's 360-minute default, so one
#    hung step holds a runner for six hours. Reusable-workflow caller jobs (`uses:`) are exempt:
#    GitHub does not allow timeout-minutes on them, and the called workflow's jobs carry it.
#    ci.yml's exact per-job values stay pinned by test-ci-job-timeouts-contract.sh.
# 2. A self-hosted job is unreachable from pull request code. This repository is public and its
#    self-hosted runner shares the owner's machine. A workflow that runs a self-hosted job must not
#    trigger on pull_request_target, and when it triggers on pull_request or workflow_call, the
#    job's `if` must start with github.event_name != 'pull_request' as a conjunct and contain no
#    `||` that could make it true anyway.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

ruby -ryaml - "${ROOT}/.github/workflows" <<'RUBY'
dir = ARGV.fetch(0)
docs = Dir.glob(File.join(dir, "*.{yml,yaml}")).sort.to_h do |path|
  [File.basename(path), YAML.safe_load_file(path, aliases: false)]
end
GUARD = "github.event_name != 'pull_request'"

def triggers(doc)
  on = doc.key?("on") ? doc["on"] : doc[true] # Psych reads a bare `on:` key as true
  on.is_a?(Hash) ? on.keys.map(&:to_s) : Array(on).map(&:to_s)
end

def self_hosted?(job)
  [job["runs-on"], job["strategy"]].to_s.include?("self-hosted")
end

def normalize(cond)
  cond.to_s.strip.sub(/\A\$\{\{\s*/, "").sub(/\s*\}\}\z/, "").gsub(/\s+/, " ")
end

def problems(docs)
  out = []
  docs.each do |name, doc|
    jobs = doc.is_a?(Hash) && doc["jobs"].is_a?(Hash) ? doc["jobs"] : {}
    on = triggers(doc)
    jobs.each do |id, job|
      next unless job.is_a?(Hash)
      unless job.key?("uses")
        t = job["timeout-minutes"]
        ok = (t.is_a?(Integer) && t.between?(1, 120)) || (t.is_a?(String) && t.strip.start_with?("${{") && t.strip.end_with?("}}"))
        out << "#{name}:#{id} needs timeout-minutes (integer 1..120 or an expression), has #{t.inspect}" unless ok
      end
      next unless self_hosted?(job)
      out << "#{name}:#{id} is self-hosted in a workflow triggered by pull_request_target" if on.include?("pull_request_target")
      next unless (on & %w[pull_request workflow_call]).any?
      cond = normalize(job["if"])
      guarded = (cond == GUARD || cond.start_with?("#{GUARD} && ")) && !cond.include?("||")
      out << "#{name}:#{id} is self-hosted and reachable from pull_request: its if must start with #{GUARD} && and contain no ||" unless guarded
    end
  end
  out
end

real = problems(docs)
unless real.empty?
  warn real.map { |p| "FAIL: #{p}" }.join("\n")
  warn "#{real.length} job hygiene problem(s)"
  exit 1
end

def mutated(docs)
  copy = Marshal.load(Marshal.dump(docs)); yield copy; copy
end
plain = lambda do |d|
  d.each_value { |doc| (doc["jobs"] || {}).each_value { |j| return j if j.is_a?(Hash) && !j.key?("uses") && !self_hosted?(j) } }
end
hosted_pr = lambda do |d|
  d.each_value do |doc|
    next unless (triggers(doc) & %w[pull_request]).any?
    (doc["jobs"] || {}).each_value { |j| return j if j.is_a?(Hash) && self_hosted?(j) }
  end
  raise "no self-hosted job in a pull_request workflow to mutate"
end
hosted_wf = lambda do |d|
  d.each_value { |doc| return doc if (doc["jobs"] || {}).values.any? { |j| j.is_a?(Hash) && self_hosted?(j) } }
end
mutations = {
  "timeout removed" => ->(d) { plain.(d).delete("timeout-minutes") },
  "timeout zero" => ->(d) { plain.(d)["timeout-minutes"] = 0 },
  "default restated" => ->(d) { plain.(d)["timeout-minutes"] = 360 },
  "timeout as plain string" => ->(d) { plain.(d)["timeout-minutes"] = "30" },
  "self-hosted guard removed" => ->(d) { hosted_pr.(d).delete("if") },
  "guard not leading" => ->(d) { j = hosted_pr.(d); j["if"] = "always() && #{GUARD}" },
  "guard with ||" => ->(d) { j = hosted_pr.(d); j["if"] = "#{GUARD} && false || true" },
  "pull_request_target added" => lambda { |d|
    doc = hosted_wf.(d); key = doc.key?("on") ? "on" : true
    doc[key] = { "pull_request_target" => nil }.merge(doc[key].is_a?(Hash) ? doc[key] : {})
  },
  "self-hosted via matrix" => lambda { |d|
    j = plain.(d); j["strategy"] = { "matrix" => { "runner" => ["self-hosted"] } }; j["runs-on"] = "${{ matrix.runner }}"
    d.each_value { |doc| doc[doc.key?("on") ? "on" : true] = { "pull_request" => nil } if (doc["jobs"] || {}).values.include?(j) }
  },
}
survived = mutations.select { |_, m| problems(mutated(docs, &m)).empty? }.keys
unless survived.empty?
  warn "FAIL: job hygiene contract did not refuse: #{survived.join(', ')}"
  exit 1
end
jobs = docs.values.sum { |doc| (doc["jobs"] || {}).size }
hosted = docs.values.sum { |doc| (doc["jobs"] || {}).values.count { |j| j.is_a?(Hash) && self_hosted?(j) } }
puts "job hygiene contract: #{jobs} jobs in #{docs.size} workflows; #{hosted} self-hosted, none reachable from pull_request; refuses #{mutations.length} neutralisations"
RUBY
