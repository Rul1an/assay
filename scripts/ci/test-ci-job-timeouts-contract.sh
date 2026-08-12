#!/usr/bin/env bash
# Contract for evidence-based timeout-minutes on every job in .github/workflows/ci.yml (CI-5B / #2244).
#
# Fourteen jobs inherited GitHub's 360-minute default. This gate pins the complete 16-job set and
# the per-job timeout class so a missing ceiling, a wrong class, or a restored 360 cannot stay green.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${1:-${ROOT}/.github/workflows/ci.yml}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

validate() {
  local wf="$1"
  [[ -f "${wf}" ]] || fail "missing workflow ${wf}"

  ruby -ryaml - "${wf}" <<'RUBY'
path = ARGV.fetch(0)
doc = YAML.safe_load_file(path, aliases: false)
abort "ci.yml must be a mapping" unless doc.is_a?(Hash)

jobs = doc.fetch("jobs")
abort "ci.yml jobs must be a mapping" unless jobs.is_a?(Hash)

# Authoritative CI-5B mapping (#2244 clarification): short=10, hosted heavy=20,
# eBPF smoke preserved at 15/60.
expected = {
  "scope" => 10,
  "clippy" => 10,
  "rustdoc" => 10,
  "public-msrv" => 10,
  "distribution-boundary" => 10,
  "publish-shape-cli" => 10,
  "public-crate-policy" => 10,
  "vendored-packs" => 10,
  "release-asset-contract" => 10,
  "mcp-registry-foundation" => 10,
  "ci" => 10,
  "deps-security" => 20,
  "perf" => 20,
  "test" => 20,
  "ebpf-smoke-ubuntu" => 15,
  "ebpf-smoke-self-hosted" => 60,
}.freeze

actual_ids = jobs.keys.map(&:to_s).sort
expected_ids = expected.keys.sort
unless actual_ids == expected_ids
  missing = expected_ids - actual_ids
  extra = actual_ids - expected_ids
  abort(
    "ci.yml job set drifted from the pinned 16-job contract; " \
    "missing=#{missing.inspect} extra=#{extra.inspect}"
  )
end

expected.each do |job_id, want|
  job = jobs.fetch(job_id)
  abort "#{job_id}: job body must be a mapping" unless job.is_a?(Hash)
  unless job.key?("timeout-minutes")
    abort "#{job_id}: missing timeout-minutes (would inherit GitHub's 360-minute default)"
  end
  got = job["timeout-minutes"]
  unless got.is_a?(Integer) && got.positive?
    abort "#{job_id}: timeout-minutes must be a positive integer literal, got #{got.inspect}"
  end
  if got == 360
    abort "#{job_id}: timeout-minutes must not be the 360-minute GitHub fallback"
  end
  unless got == want
    abort "#{job_id}: timeout-minutes class mismatch: expected #{want}, got #{got}"
  end
end

rollup = jobs.fetch("ci")
expected_needs = %w[
  scope
  deps-security
  clippy
  rustdoc
  public-msrv
  distribution-boundary
  publish-shape-cli
  public-crate-policy
  vendored-packs
  release-asset-contract
  mcp-registry-foundation
  perf
  test
  ebpf-smoke-ubuntu
]
got_needs = Array(rollup["needs"]).map(&:to_s)
unless got_needs == expected_needs
  abort "CI rollup needs drifted; expected #{expected_needs.inspect}, got #{got_needs.inspect}"
end
unless rollup["if"] == "always()"
  abort "CI rollup must stay fail-closed with if: always(); got #{rollup['if'].inspect}"
end

puts "ci-job-timeouts contract=passed (16 jobs; rollup bounded at 10m)"
RUBY
}

self_test() {
  validate "$WORKFLOW" >/dev/null
  tmp="$(mktemp -d)"
  trap 'rm -rf "$tmp"' EXIT

  run_mutation() {
    local name="$1"
    local ruby_mutation="$2"
    local mutated="${tmp}/${name}.yml"
    cp "$WORKFLOW" "$mutated"
    ruby -ryaml - "$mutated" "$ruby_mutation" <<'RUBY'
path, mutation = ARGV
doc = YAML.safe_load_file(path, aliases: false)
jobs = doc.fetch("jobs")
case mutation
when "missing-timeout"
  jobs.fetch("scope").delete("timeout-minutes")
when "wrong-class-short-as-heavy"
  jobs.fetch("scope")["timeout-minutes"] = 20
when "wrong-class-heavy-as-short"
  jobs.fetch("test")["timeout-minutes"] = 10
when "fallback-360"
  jobs.fetch("ci")["timeout-minutes"] = 360
when "ebpf-ubuntu-changed"
  jobs.fetch("ebpf-smoke-ubuntu")["timeout-minutes"] = 30
when "ebpf-self-hosted-changed"
  jobs.fetch("ebpf-smoke-self-hosted")["timeout-minutes"] = 90
when "unpinned-new-job"
  jobs["rogue-unpinned"] = {
    "name" => "Rogue",
    "runs-on" => "ubuntu-latest",
    "timeout-minutes" => 10,
    "steps" => [{"run" => "true"}],
  }
else
  abort "unknown mutation #{mutation}"
end
File.write(path, YAML.dump(doc))
RUBY
    if validate "$mutated" >/dev/null 2>&1; then
      echo "mutation did not bite: $name" >&2
      exit 1
    fi
    echo "ok    $name"
  }

  run_mutation missing-timeout missing-timeout
  run_mutation wrong-class-short-as-heavy wrong-class-short-as-heavy
  run_mutation wrong-class-heavy-as-short wrong-class-heavy-as-short
  run_mutation fallback-360 fallback-360
  run_mutation ebpf-ubuntu-changed ebpf-ubuntu-changed
  run_mutation ebpf-self-hosted-changed ebpf-self-hosted-changed
  run_mutation unpinned-new-job unpinned-new-job
  echo "ci-job-timeouts contract self-test=passed"
}

if [[ "${2:-}" == "--self-test" || "${1:-}" == "--self-test" ]]; then
  [[ "${1:-}" == "--self-test" ]] && WORKFLOW="${ROOT}/.github/workflows/ci.yml"
  self_test
else
  validate "$WORKFLOW"
fi
