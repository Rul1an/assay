#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${1:-${ROOT}/.github/workflows/host-capability-proof.yml}"
CACHE_PIN="6323deb102c322ba6fcbdcafc7e3dddab59af2b6"
TOOLCHAIN_PIN="29eef336d9b2848a0b548edc03f92a220660cdb8"

validate() {
  ruby -ryaml - "$1" "$CACHE_PIN" "$TOOLCHAIN_PIN" <<'RUBY'
path, cache_pin, toolchain_pin = ARGV
doc = YAML.safe_load_file(path, aliases: false)
abort "host proof workflow must be a mapping" unless doc.is_a?(Hash)

permissions = doc.fetch("permissions")
expected_permissions = {"contents" => "read", "actions" => "read"}
abort "host proof permissions must stay read-only" unless permissions == expected_permissions

job = doc.fetch("jobs").fetch("proof")
abort "host proof cold-start timeout must be 90 minutes" unless job["timeout-minutes"] == 90
steps = job.fetch("steps")

named = steps.each_with_object({}) do |step, out|
  name = step["name"]
  abort "host proof step is missing a name" unless name.is_a?(String) && !name.empty?
  abort "duplicate host proof step #{name.inspect}" if out.key?(name)
  out[name] = step
end

required = [
  "Checkout repository",
  "Resolve pinned Rust toolchain",
  "Install pinned Rust toolchain",
  "Restore bounded Rust build cache",
  "Build assay CLI",
  "Run doctor and collect provenance",
  "Upload proof artifact",
]
missing = required.reject { |name| named.key?(name) }
abort "host proof steps missing: #{missing.join(', ')}" unless missing.empty?

positions = required.map { |name| steps.index(named.fetch(name)) }
abort "host proof checkout/cache/build/doctor/upload order drifted" unless positions == positions.sort

cache = named.fetch("Restore bounded Rust build cache")
expected_use = "Swatinem/rust-cache@#{cache_pin}"
abort "host proof rust-cache pin drifted" unless cache["uses"] == expected_use
cache_with = cache.fetch("with")
abort "host proof rust-cache key is not isolated" unless cache_with["prefix-key"] == "host-capability-proof-v2"
abort "host proof cache must not mutate Cargo toolchain binaries" unless cache_with["cache-bin"] == false || cache_with["cache-bin"] == "false"
abort "host proof must not save failed builds" if cache_with["cache-on-failure"] == true || cache_with["cache-on-failure"] == "true"

resolve = named.fetch("Resolve pinned Rust toolchain")
abort "host proof toolchain resolver needs id rust-toolchain" unless resolve["id"] == "rust-toolchain"
resolve_run = resolve.fetch("run")
abort "host proof toolchain must be read from rust-toolchain.toml" unless resolve_run.include?("rust-toolchain.toml")
abort "host proof toolchain resolver must publish an output" unless resolve_run.include?("GITHUB_OUTPUT")

install = named.fetch("Install pinned Rust toolchain")
expected_toolchain_use = "dtolnay/rust-toolchain@#{toolchain_pin}"
abort "host proof rust-toolchain action pin drifted" unless install["uses"] == expected_toolchain_use
expected_toolchain = "${{ steps.rust-toolchain.outputs.channel }}"
abort "host proof install must consume the repository toolchain pin" unless install.fetch("with")["toolchain"] == expected_toolchain

["Restore bounded Rust build cache", "Build assay CLI", "Upload proof artifact"].each do |name|
  value = named.fetch(name)["continue-on-error"]
  abort "#{name} must stay fail-closed" if value == true || value == "true"
end

upload = named.fetch("Upload proof artifact")
abort "host proof upload must fail when proof files are absent" unless upload.fetch("with")["if-no-files-found"] == "error"

puts "host-capability-proof contract=passed"
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
job = doc.fetch("jobs").fetch("proof")
steps = job.fetch("steps")
case mutation
when "remove-cache"
  steps.reject! { |step| step["name"] == "Restore bounded Rust build cache" }
when "float-cache"
  steps.find { |step| step["name"] == "Restore bounded Rust build cache" }["uses"] = "Swatinem/rust-cache@v2"
when "share-cache"
  steps.find { |step| step["name"] == "Restore bounded Rust build cache" }.fetch("with")["prefix-key"] = "v0-rust"
when "late-cache"
  cache = steps.find { |step| step["name"] == "Restore bounded Rust build cache" }
  steps.delete(cache)
  build_index = steps.index { |step| step["name"] == "Build assay CLI" }
  steps.insert(build_index + 1, cache)
when "cache-failure"
  steps.find { |step| step["name"] == "Restore bounded Rust build cache" }.fetch("with")["cache-on-failure"] = true
when "cache-bin"
  steps.find { |step| step["name"] == "Restore bounded Rust build cache" }.fetch("with")["cache-bin"] = true
when "remove-toolchain"
  steps.reject! { |step| ["Resolve pinned Rust toolchain", "Install pinned Rust toolchain"].include?(step["name"]) }
when "float-toolchain"
  steps.find { |step| step["name"] == "Install pinned Rust toolchain" }["uses"] = "dtolnay/rust-toolchain@stable"
when "hardcode-toolchain"
  steps.find { |step| step["name"] == "Install pinned Rust toolchain" }.fetch("with")["toolchain"] = "1.96.0"
when "shrink-timeout"
  job["timeout-minutes"] = 30
when "widen-permissions"
  doc.fetch("permissions")["actions"] = "write"
when "soft-cache"
  steps.find { |step| step["name"] == "Restore bounded Rust build cache" }["continue-on-error"] = true
when "soft-build"
  steps.find { |step| step["name"] == "Build assay CLI" }["continue-on-error"] = true
when "soft-upload"
  steps.find { |step| step["name"] == "Upload proof artifact" }.fetch("with")["if-no-files-found"] = "warn"
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

  run_mutation remove-cache remove-cache
  run_mutation float-cache float-cache
  run_mutation share-cache share-cache
  run_mutation late-cache late-cache
  run_mutation cache-failure cache-failure
  run_mutation cache-bin cache-bin
  run_mutation remove-toolchain remove-toolchain
  run_mutation float-toolchain float-toolchain
  run_mutation hardcode-toolchain hardcode-toolchain
  run_mutation shrink-timeout shrink-timeout
  run_mutation widen-permissions widen-permissions
  run_mutation soft-cache soft-cache
  run_mutation soft-build soft-build
  run_mutation soft-upload soft-upload
  echo "host-capability-proof contract self-test=passed"
}

if [[ "${2:-}" == "--self-test" || "${1:-}" == "--self-test" ]]; then
  [[ "${1:-}" == "--self-test" ]] && WORKFLOW="${ROOT}/.github/workflows/host-capability-proof.yml"
  self_test
else
  validate "$WORKFLOW"
fi
