#!/usr/bin/env bash
# Contract: the two checks that compare install pins with live published state run only in
# .github/workflows/release-pin-drift.yml, on a schedule and on demand, and cannot be silently
# disabled there.
#
# They used to run in the required CI aggregate, where a release made every commit red until the
# pin promotion merged. This contract keeps them out of every other workflow (so they cannot drift
# back into a required gate) and pins the drift workflow's trigger, permissions and exact run
# step in the parsed YAML. The checker's own offline self-tests are not affected and stay in CI.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

ruby -ryaml - "${ROOT}/.github/workflows" <<'RUBY'
dir = ARGV.fetch(0)
DRIFT = "release-pin-drift.yml"
LIVE = [
  "bash scripts/ci/check-assay-release-pin.sh --published",
  "bash scripts/ci/check-assay-action-pin.sh --published",
].freeze
RUN = ["set -euo pipefail", *LIVE].freeze

docs = Dir.glob(File.join(dir, "*.{yml,yaml}")).sort.to_h do |path|
  [File.basename(path), YAML.safe_load_file(path, aliases: false)]
end

def run_lines(doc)
  jobs = doc.is_a?(Hash) && doc["jobs"].is_a?(Hash) ? doc["jobs"] : {}
  jobs.values.flat_map do |job|
    steps = job.is_a?(Hash) && job["steps"].is_a?(Array) ? job["steps"] : []
    steps.flat_map { |s| s.is_a?(Hash) && s["run"].is_a?(String) ? s["run"].lines.map(&:strip) : [] }
  end
end

def problems(docs)
  out = []
  docs.each do |name, doc|
    next if name == DRIFT
    hits = run_lines(doc).grep(/check-assay-(?:release|action)-pin\.sh\b.*--published\b/)
    out << "#{name} runs a live published-pin check (#{hits.first}); it belongs only in #{DRIFT}" unless hits.empty?
  end
  doc = docs[DRIFT]
  return out + ["#{DRIFT} is missing"] unless doc.is_a?(Hash)
  # Psych reads a bare `on:` key as true.
  on = doc.key?("on") ? doc["on"] : doc[true]
  unless on.is_a?(Hash) && on.keys.sort == %w[schedule workflow_dispatch]
    out << "#{DRIFT} must trigger on exactly schedule and workflow_dispatch"
  end
  if on.is_a?(Hash)
    crons = Array(on["schedule"]).map { |e| e.is_a?(Hash) ? e["cron"] : nil }
    out << "#{DRIFT} needs at least one cron schedule" if crons.empty? || crons.any? { |c| !c.is_a?(String) || c.strip.empty? }
  end
  out << "#{DRIFT} workflow permissions must be {}" unless doc["permissions"] == {}
  jobs = doc["jobs"]
  return out + ["#{DRIFT} must have exactly one job, published-pins"] unless jobs.is_a?(Hash) && jobs.keys == ["published-pins"]
  job = jobs["published-pins"]
  out << "published-pins must only guard on the repository" unless job["if"] == "github.repository == 'Rul1an/assay'"
  out << "published-pins permissions must be exactly contents: read" unless job["permissions"] == { "contents" => "read" }
  out << "published-pins needs an integer timeout-minutes of at most 15" unless job["timeout-minutes"].is_a?(Integer) && job["timeout-minutes"].between?(1, 15)
  %w[continue-on-error needs strategy].each { |k| out << "published-pins must not set #{k}" if job.key?(k) }
  steps = job["steps"].is_a?(Array) ? job["steps"] : []
  runs = steps.select { |s| s.is_a?(Hash) && s.key?("run") }
  if runs.length != 1
    out << "published-pins must have exactly one run step"
  else
    s = runs.first
    lines = s["run"].to_s.lines.map(&:strip).reject(&:empty?)
    out << "published-pins must run exactly: #{RUN.join(' / ')}" unless lines == RUN
    out << "published-pins run step env must be exactly GH_TOKEN from github.token" unless s["env"] == { "GH_TOKEN" => "${{ github.token }}" }
    out << "published-pins run step must use bash" unless s["shell"] == "bash"
    %w[if continue-on-error].each { |k| out << "published-pins run step must not set #{k}" if s.key?(k) }
  end
  checkout = steps.find { |s| s.is_a?(Hash) && s["uses"].to_s.start_with?("actions/checkout@") }
  out << "published-pins checkout must not persist credentials" unless checkout && checkout["with"].is_a?(Hash) && checkout["with"]["persist-credentials"] == false
  out
end

real = problems(docs)
unless real.empty?
  warn real.map { |p| "FAIL: #{p}" }.join("\n")
  exit 1
end

def mutated(docs)
  copy = Marshal.load(Marshal.dump(docs)); yield copy; copy
end
job = ->(d) { d[DRIFT]["jobs"]["published-pins"] }
step = ->(d) { job.(d)["steps"].find { |s| s.key?("run") } }
on = ->(d) { d[DRIFT].key?("on") ? d[DRIFT]["on"] : d[DRIFT][true] }
ci_step = lambda do |d|
  d["ci.yml"]["jobs"].values.flat_map { |j| j["steps"] || [] }.find { |s| s.is_a?(Hash) && s["run"].is_a?(String) }
end
mutations = {
  "release pin check back in ci.yml" => ->(d) { ci_step.(d)["run"] += "\nbash scripts/ci/check-assay-release-pin.sh --published\n" },
  "action pin check in another workflow" => ->(d) { ci_step.(d)["run"] += "\n  bash scripts/ci/check-assay-action-pin.sh --published || true\n" },
  "respelled call in another workflow" => ->(d) { ci_step.(d)["run"] += "\nbash ./scripts/ci/check-assay-release-pin.sh  --published\n" },
  "drift workflow removed" => ->(d) { d.delete(DRIFT) },
  "schedule removed" => ->(d) { on.(d).delete("schedule") },
  "empty cron" => ->(d) { on.(d)["schedule"] = [{ "cron" => " " }] },
  "pull_request trigger added" => ->(d) { on.(d)["pull_request"] = nil },
  "one live check dropped" => ->(d) { step.(d)["run"] = "set -euo pipefail\nbash scripts/ci/check-assay-release-pin.sh --published\n" },
  "run with || true" => ->(d) { step.(d)["run"] = step.(d)["run"].sub("--published\n", "--published || true\n") },
  "job continue-on-error" => ->(d) { job.(d)["continue-on-error"] = true },
  "step if: false" => ->(d) { step.(d)["if"] = false },
  "job if: false" => ->(d) { job.(d)["if"] = false },
  "write permissions" => ->(d) { job.(d)["permissions"] = { "contents" => "write" } },
  "token dropped" => ->(d) { step.(d).delete("env") },
  "credentials persisted" => ->(d) { job.(d)["steps"].find { |s| s["uses"].to_s.start_with?("actions/checkout@") }["with"]["persist-credentials"] = true },
  "timeout removed" => ->(d) { job.(d).delete("timeout-minutes") },
}
survived = mutations.select { |_, m| problems(mutated(docs, &m)).empty? }.keys
unless survived.empty?
  warn "FAIL: release-pin drift contract did not refuse: #{survived.join(', ')}"
  exit 1
end
puts "release-pin drift contract: live checks only in #{DRIFT}; refuses #{mutations.length} neutralisations"
RUBY
