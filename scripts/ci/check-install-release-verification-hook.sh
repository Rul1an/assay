#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG="${PRECOMMIT_CONFIG:-$ROOT/.pre-commit-config.yaml}"

ruby -EUTF-8:UTF-8 - "$CONFIG" <<'RUBY'
require "yaml"

config = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
repos = config.is_a?(Hash) ? config["repos"] : nil
abort "pre-commit config has no repos list" unless repos.is_a?(Array)

expected = {
  "install-release-verification-self-test" => {
    "entry" => "bash scripts/ci/test-install-release-verification.sh",
    "language" => "system",
    "pass_filenames" => false,
    "stages" => ["pre-commit"],
    "files" => "^(scripts/install\\.sh|scripts/ci/(check-install-release-verification-hook|test-install-release-verification)\\.sh|README\\.md|\\.pre-commit-config\\.yaml)$",
  },
  "install-release-verification-hook-contract" => {
    "entry" => "bash scripts/ci/check-install-release-verification-hook.sh",
    "language" => "system",
    "pass_filenames" => false,
    "always_run" => true,
    "stages" => ["pre-commit"],
  },
}

found = Hash.new { |hash, key| hash[key] = [] }
repos.each do |repo|
  next unless repo.is_a?(Hash)
  hooks = repo["hooks"]
  next unless hooks.is_a?(Array)
  hooks.each do |hook|
    next unless hook.is_a?(Hash) && expected.key?(hook["id"])
    found[hook["id"]] << [repo["repo"], hook]
  end
end

expected.each do |hook_id, required|
  rows = found.fetch(hook_id)
  abort "expected exactly one #{hook_id}, found #{rows.length}" unless rows.length == 1
  owner, hook = rows.fetch(0)
  abort "#{hook_id} must be owned by repo: local" unless owner == "local"
  required.each do |key, value|
    actual = hook[key]
    abort "#{hook_id} #{key}=#{actual.inspect}, expected #{value.inspect}" unless actual == value
  end
end

puts "install-release-verification-hook=pass"
RUBY
