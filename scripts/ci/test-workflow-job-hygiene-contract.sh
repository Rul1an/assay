#!/usr/bin/env bash
# Contract: two hygiene rules for every job in every workflow under .github/workflows.
#
# 1. Every job states its own timeout-minutes: an integer from 1 to 120, or a ${{ }} expression
#    (a matrix-dependent ceiling). A job without one inherits GitHub's 360-minute default, so one
#    hung step holds a runner for six hours. Reusable-workflow caller jobs (`uses:`) are exempt:
#    GitHub does not allow timeout-minutes on them, and the called workflow's jobs carry it.
#    ci.yml's exact per-job values stay pinned by test-ci-job-timeouts-contract.sh.
# 2. A job whose runner selector names `self-hosted` is unreachable from pull request code. This
#    repository is public and its self-hosted runner shares the owner's machine. A workflow that
#    runs such a job must not trigger on pull_request_target, and when it triggers on
#    pull_request or workflow_call, the job's `if` must start with
#    github.event_name != 'pull_request' as a conjunct and contain no `||`.
#
# What rule 2 can and cannot see. It reads runner selectors as written. A selector is HOSTED when
# every label is one of the GitHub-hosted image labels this repository uses, listed exactly in
# HOSTED_LABELS (a name with an OS prefix is not enough: a self-hosted runner can carry a custom
# label such as ubuntu-box), resolving `${{ matrix.<key> }}` only through a literal matrix in the
# same job; it is SELF-HOSTED when a label is `self-hosted`. Everything else is UNRESOLVED: other
# labels (they can still select a self-hosted runner), runner groups, non-literal matrices, other
# expressions, and remote reusable workflows, whose runners the called repository chooses. The
# check cannot prove where an unresolved job runs, so it does not claim to. Each one must be listed
# in UNRESOLVED with its selector and a reason: a new one fails until someone reviews and lists it,
# a listed job whose selector changed fails until it is reviewed again, and a listed one that no
# longer exists fails as stale. A remote workflow is bound by its path without the @ref, so a pin
# bump passes and a different workflow does not. It also trusts that a listed GitHub-hosted image
# label routes to a GitHub-hosted runner; adding a label to HOSTED_LABELS is a reviewed change. Local reusable-workflow callers are not judged themselves: the called
# file's jobs are checked in that file, under its workflow_call trigger.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

ruby -ryaml -rjson - "${ROOT}/.github/workflows" <<'RUBY'
dir = ARGV.fetch(0)
docs = Dir.glob(File.join(dir, "*.{yml,yaml}")).sort.to_h do |path|
  [File.basename(path), YAML.safe_load_file(path, aliases: false)]
end
GUARD = "github.event_name != 'pull_request'"
# The GitHub-hosted image labels the workflows use today, measured from the YAML on 2026-11-26.
HOSTED_LABELS = %w[
  macos-15 macos-26 macos-26-intel macos-latest
  ubuntu-24.04 ubuntu-24.04-arm
  windows-2025 windows-latest
].freeze
# Reviewed selectors this check cannot resolve, keyed "<workflow file>:<job id>", each bound to the
# selector it was reviewed for (see signature below).
UNRESOLVED = {
  "ci.yml:osv-cargo-lock" => {
    "selector" => "uses:google/osv-scanner-action/.github/workflows/osv-scanner-reusable.yml",
    "reason" => "remote reusable workflow, SHA-pinned; the called repository chooses its runners",
  },
  "python-published-doc-consumer.yml:consumer" => {
    "selector" => "runs-on:${{ matrix.os }} matrix:{\"include\":\"${{ fromJSON(needs.plan.outputs.wheels) }}\"}",
    "reason" => "the os values come from scripts/ci/plan-python-artifact-matrix.py, which reads " \
                "assay-python-sdk/python-artifact-matrix.v0.json at run time",
  },
  "release.yml:wheels" => {
    "selector" => "runs-on:${{ matrix.os }} matrix:{\"include\":\"${{ fromJSON(needs.plan-python-artifact.outputs.wheels) }}\"}",
    "reason" => "the os values come from scripts/ci/plan-python-artifact-matrix.py, which reads " \
                "assay-python-sdk/python-artifact-matrix.v0.json at run time",
  },
}.freeze

# What an UNRESOLVED entry is bound to: a remote workflow's path without its @ref, or the
# job's runs-on together with its matrix.
def signature(job)
  return "uses:#{job["uses"].to_s.sub(/@[^@]*\z/, "")}" if job.key?("uses")
  # JSON, not Hash#inspect, whose format differs between Ruby versions.
  "runs-on:#{job["runs-on"]} matrix:#{JSON.generate(job.dig("strategy", "matrix"))}"
end

def triggers(doc)
  on = doc.key?("on") ? doc["on"] : doc[true] # Psych reads a bare `on:` key as true
  on.is_a?(Hash) ? on.keys.map(&:to_s) : Array(on).map(&:to_s)
end

# [kind, detail] with kind one of :hosted, :self_hosted, :unresolved, :local_caller.
def selector(job)
  if job.key?("uses")
    uses = job["uses"].to_s
    return [:local_caller, uses] if uses.start_with?("./")
    return [:unresolved, "remote reusable workflow #{uses}"]
  end
  ro = job["runs-on"]
  values =
    if ro.is_a?(String) && (m = ro.strip.match(/\A\$\{\{\s*matrix\.([A-Za-z0-9_-]+)\s*\}\}\z/))
      key = m[1]
      matrix = job.dig("strategy", "matrix")
      return [:unresolved, "runs-on #{ro} without a literal matrix"] unless matrix.is_a?(Hash)
      found = []
      found.concat(Array(matrix[key])) if matrix.key?(key)
      Array(matrix["include"]).each { |entry| found << entry[key] if entry.is_a?(Hash) && entry.key?(key) }
      return [:unresolved, "runs-on #{ro} with no literal #{key} values"] if found.empty?
      found
    elsif ro.is_a?(String)
      [ro]
    elsif ro.is_a?(Array)
      ro
    else
      return [:unresolved, "runs-on #{ro.inspect} (a runner group or mapping)"]
    end
  labels = values.flat_map { |v| v.is_a?(Array) ? v : [v] }
  return [:self_hosted, labels] if labels.any? { |l| l == "self-hosted" }
  return [:hosted, labels] if labels.all? { |l| HOSTED_LABELS.include?(l) }
  [:unresolved, "runs-on labels #{labels.inspect} are not all listed GitHub-hosted image labels"]
end

def jobs_of(doc)
  doc.is_a?(Hash) && doc["jobs"].is_a?(Hash) ? doc["jobs"].select { |_, j| j.is_a?(Hash) } : {}
end

def problems(docs)
  out = []
  unresolved = {}
  docs.each do |name, doc|
    on = triggers(doc)
    jobs_of(doc).each do |id, job|
      unless job.key?("uses")
        t = job["timeout-minutes"]
        ok = (t.is_a?(Integer) && t.between?(1, 120)) || (t.is_a?(String) && t.strip.start_with?("${{") && t.strip.end_with?("}}"))
        out << "#{name}:#{id} needs timeout-minutes (integer 1..120 or an expression), has #{t.inspect}" unless ok
      end
      kind, detail = selector(job)
      if kind == :unresolved
        unresolved["#{name}:#{id}"] = detail
        listed = UNRESOLVED["#{name}:#{id}"]
        if listed && listed["selector"] != signature(job)
          out << "#{name}:#{id} changed its runner selector to #{signature(job)}; it was reviewed for #{listed["selector"]}, review it again"
        end
      end
      next unless kind == :self_hosted
      out << "#{name}:#{id} is self-hosted in a workflow triggered by pull_request_target" if on.include?("pull_request_target")
      next unless (on & %w[pull_request workflow_call]).any?
      cond = job["if"].to_s.strip.sub(/\A\$\{\{\s*/, "").sub(/\s*\}\}\z/, "").gsub(/\s+/, " ")
      guarded = (cond == GUARD || cond.start_with?("#{GUARD} && ")) && !cond.include?("||")
      out << "#{name}:#{id} is self-hosted and reachable from pull_request: its if must start with #{GUARD} && and contain no ||" unless guarded
    end
  end
  (unresolved.keys - UNRESOLVED.keys).each do |key|
    out << "#{key} has a runner selector this check cannot resolve (#{unresolved[key]}); review where it runs and list it in UNRESOLVED with a reason"
  end
  (UNRESOLVED.keys - unresolved.keys).each { |key| out << "UNRESOLVED lists #{key}, which is no longer an unresolved selector; remove it" }
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
def first_job(d, kind, trigger = nil)
  d.each_value do |doc|
    next if trigger && !triggers(doc).include?(trigger)
    jobs_of(doc).each_value { |j| return j if selector(j).first == kind }
  end
  raise "no #{kind} job#{trigger ? " in a #{trigger} workflow" : ''} to mutate"
end
hosted_pr = ->(d) { first_job(d, :hosted, "pull_request") }
self_hosted_pr = ->(d) { first_job(d, :self_hosted, "pull_request") }
mutations = {
  "timeout removed" => ->(d) { first_job(d, :hosted).delete("timeout-minutes") },
  "timeout zero" => ->(d) { first_job(d, :hosted)["timeout-minutes"] = 0 },
  "default restated" => ->(d) { first_job(d, :hosted)["timeout-minutes"] = 360 },
  "timeout as plain string" => ->(d) { first_job(d, :hosted)["timeout-minutes"] = "30" },
  "self-hosted guard removed" => ->(d) { self_hosted_pr.(d).delete("if") },
  "guard not leading" => ->(d) { self_hosted_pr.(d)["if"] = "always() && #{GUARD}" },
  "guard with ||" => ->(d) { self_hosted_pr.(d)["if"] = "#{GUARD} && false || true" },
  "pull_request_target added" => lambda { |d|
    doc = d.values.find { |w| jobs_of(w).values.any? { |j| selector(j).first == :self_hosted } }
    key = doc.key?("on") ? "on" : true
    doc[key] = { "pull_request_target" => nil }.merge(doc[key].is_a?(Hash) ? doc[key] : {})
  },
  "self-hosted via literal matrix" => lambda { |d|
    j = hosted_pr.(d); j["strategy"] = { "matrix" => { "runner" => [["self-hosted", "linux"]] } }; j["runs-on"] = "${{ matrix.runner }}"; j.delete("if")
  },
  "custom labels without self-hosted" => ->(d) { hosted_pr.(d)["runs-on"] = ["linux", "assay-bpf-runner"] },
  "runner group" => ->(d) { hosted_pr.(d)["runs-on"] = { "group" => "owner-machines" } },
  "matrix with an unknown label" => lambda { |d|
    j = hosted_pr.(d); j["strategy"] = { "matrix" => { "os" => ["ubuntu-24.04", "build-box"] } }; j["runs-on"] = "${{ matrix.os }}"
  },
  "non-literal matrix" => lambda { |d|
    j = hosted_pr.(d); j["strategy"] = { "matrix" => "${{ fromJSON(inputs.matrix) }}" }; j["runs-on"] = "${{ matrix.os }}"
  },
  "runs-on from an input" => ->(d) { hosted_pr.(d)["runs-on"] = "${{ inputs.runner }}" },
  "unknown label with an OS prefix" => ->(d) { hosted_pr.(d)["runs-on"] = "ubuntu-selfhosted-box" },
  "listed remote workflow replaced" => ->(d) { d["ci.yml"]["jobs"]["osv-cargo-lock"]["uses"] = "example/other/.github/workflows/scan.yml@v1" },
  "listed wheel matrix source replaced" => ->(d) { d["release.yml"]["jobs"]["wheels"]["strategy"]["matrix"]["include"] = "${{ fromJSON(inputs.wheels) }}" },
  "stale unresolved entry" => ->(d) { d["ci.yml"]["jobs"]["osv-cargo-lock"]["uses"] = "./.github/workflows/osv-local.yml" },
}
survived = mutations.select { |_, m| problems(mutated(docs, &m)).empty? }.keys
unless survived.empty?
  warn "FAIL: job hygiene contract did not refuse: #{survived.join(', ')}"
  exit 1
end
tally = Hash.new(0)
docs.each_value { |doc| jobs_of(doc).each_value { |j| tally[selector(j).first] += 1 } }
total = tally.values.sum
puts "job hygiene contract: #{total} jobs in #{docs.size} workflows; runner selectors as written: " \
     "#{tally[:hosted]} GitHub-hosted image labels, #{tally[:self_hosted]} self-hosted (none reachable from pull_request), " \
     "#{tally[:local_caller]} local reusable-workflow callers, #{tally[:unresolved]} unresolved (not proven, listed below); " \
     "refuses #{mutations.length} neutralisations"
# Positive control: bumping the listed remote workflow's pin is not a new selector.
bumped = mutated(docs) { |d| d["ci.yml"]["jobs"]["osv-cargo-lock"]["uses"] = d["ci.yml"]["jobs"]["osv-cargo-lock"]["uses"].sub(/@[^@]*\z/, "@" + "0" * 40) }
unless problems(bumped).empty?
  warn "FAIL: a pin bump of a listed remote workflow was refused: #{problems(bumped).join('; ')}"
  exit 1
end
UNRESOLVED.each { |key, entry| puts "  unresolved #{key}: #{entry["selector"]} (#{entry["reason"]})" }
RUBY
