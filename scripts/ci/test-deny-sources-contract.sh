#!/usr/bin/env bash
# Contract for deny-sources.toml: source provenance over the whole workspace.
#
# deny.toml excludes assay-ebpf (and two private crates) from its graph, so its [sources] section
# never sees the Aya git dependencies: unknown-git = "deny" there exits 0 on the current graph, and
# an allow-git entry there is never matched (#1191 pruned one as stale). deny-sources.toml checks
# sources only, over every workspace crate and dev dependency, refuses any git source it does not
# list, and lists exactly the official Aya repository.
#
#   --static     (required CI hardening step) pins the parsed deny-sources.toml and the parsed
#                ci.yml steps that run it; no cargo-deny needed.
#   --behaviour  (deps-security job, after the pinned cargo-deny install) runs cargo-deny: the real
#                config exits 0, and the same config without allow-git must refuse the Aya sources,
#                which proves the graph it checks contains assay-ebpf.
#
# Not claimed: allow-git is URL-level, so this does not pin which Aya tag or commit is used (the
# lockfile and scripts/ci/check_aya_alignment.py do), and it says nothing about advisories, which
# stay on deny.toml's graph.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONFIG="${ROOT}/deny-sources.toml"
MODE="${1:---static}"
AYA="https://github.com/aya-rs/aya"

static() {
  python3 - "${CONFIG}" "${AYA}" <<'PY'
import copy, sys, tomllib
path, aya = sys.argv[1], sys.argv[2]
try:
    with open(path, "rb") as handle:
        doc = tomllib.load(handle)
except FileNotFoundError:
    sys.exit(f"FAIL: {path} is missing")

EXPECTED_SOURCES = {
    "unknown-registry": "deny",
    "unknown-git": "deny",
    "allow-registry": ["https://github.com/rust-lang/crates.io-index"],
    "allow-git": [aya],
}

def problems(doc):
    out = []
    if set(doc) != {"graph", "sources"}:
        out.append(f"deny-sources.toml must hold only [graph] and [sources], has {sorted(doc)}")
    graph = doc.get("graph", {})
    if graph != {"exclude-dev": False}:
        out.append(f"[graph] must be exactly exclude-dev = false (no exclude, no targets), has {graph}")
    if doc.get("sources") != EXPECTED_SOURCES:
        out.append(f"[sources] must be exactly {EXPECTED_SOURCES}, has {doc.get('sources')}")
    return out

real = problems(doc)
if real:
    sys.exit("\n".join(f"FAIL: {p}" for p in real))

def mutate(fn):
    copy_doc = copy.deepcopy(doc)
    fn(copy_doc)
    return copy_doc

mutations = {
    "assay-ebpf excluded": lambda d: d["graph"].__setitem__("exclude", ["assay-ebpf"]),
    "dev dependencies excluded": lambda d: d["graph"].__setitem__("exclude-dev", True),
    "graph narrowed to targets": lambda d: d["graph"].__setitem__("targets", ["x86_64-unknown-linux-gnu"]),
    "unknown git only warns": lambda d: d["sources"].__setitem__("unknown-git", "warn"),
    "unknown registry only warns": lambda d: d["sources"].__setitem__("unknown-registry", "warn"),
    "second git source allowed": lambda d: d["sources"]["allow-git"].append("https://github.com/example/fork"),
    "whole org allowed": lambda d: d["sources"].__setitem__("allow-org", {"github": ["aya-rs"]}),
    "advisories section added": lambda d: d.__setitem__("advisories", {"ignore": []}),
}
survived = [name for name, fn in mutations.items() if not problems(mutate(fn))]
if survived:
    sys.exit(f"FAIL: deny-sources.toml pin did not refuse: {', '.join(survived)}")
print(f"deny-sources.toml pin: exact graph and sources; refuses {len(mutations)} neutralisations")
PY

  ruby -ryaml - "${ROOT}/.github/workflows/ci.yml" <<'RUBY'
doc = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
CHECK = "cargo-deny --config deny-sources.toml check sources"
BEHAVIOUR = "bash scripts/ci/test-deny-sources-contract.sh --behaviour"

def problems(doc)
  job = doc.dig("jobs", "deps-security")
  return ["ci.yml has no deps-security job"] unless job.is_a?(Hash)
  out = []
  out << "deps-security must not set continue-on-error" if job.key?("continue-on-error")
  steps = job["steps"].is_a?(Array) ? job["steps"] : []
  [CHECK, BEHAVIOUR].each do |cmd|
    hits = steps.select { |s| s.is_a?(Hash) && s["run"].is_a?(String) && s["run"].strip == cmd }
    if hits.length != 1
      out << "deps-security must have exactly one step whose run is #{cmd}"
    else
      %w[if continue-on-error].each { |k| out << "the #{cmd} step must not set #{k}" if hits.first.key?(k) }
    end
  end
  install = steps.index { |s| s.is_a?(Hash) && s["run"].to_s.include?("cargo install") && s["run"].to_s.include?("cargo-deny") }
  check = steps.index { |s| s.is_a?(Hash) && s["run"].to_s.strip == CHECK }
  out << "the sources check must run after the pinned cargo-deny install" if install.nil? || check.nil? || check < install
  out
end

real = problems(doc)
abort(real.map { |p| "FAIL: #{p}" }.join("\n")) unless real.empty?
def mutated(doc)
  copy = Marshal.load(Marshal.dump(doc)); yield copy; copy
end
step = ->(d, cmd) { d["jobs"]["deps-security"]["steps"].find { |s| s["run"].to_s.strip == cmd } }
mutations = {
  "check step removed" => ->(d) { d["jobs"]["deps-security"]["steps"].delete(step.(d, CHECK)) },
  "behaviour step removed" => ->(d) { d["jobs"]["deps-security"]["steps"].delete(step.(d, BEHAVIOUR)) },
  "check with || true" => ->(d) { step.(d, CHECK)["run"] = "#{CHECK} || true" },
  "config flag dropped" => ->(d) { step.(d, CHECK)["run"] = "cargo-deny check sources" },
  "check step if: false" => ->(d) { step.(d, CHECK)["if"] = false },
  "behaviour continue-on-error" => ->(d) { step.(d, BEHAVIOUR)["continue-on-error"] = true },
  "job continue-on-error" => ->(d) { d["jobs"]["deps-security"]["continue-on-error"] = true },
}
survived = mutations.select { |_, m| problems(mutated(doc, &m)).empty? }.keys
abort("FAIL: deps-security call-site pin did not refuse: #{survived.join(', ')}") unless survived.empty?
puts "deps-security call-site pin: sources check and behaviour test; refuses #{mutations.length} neutralisations"
RUBY
}

behaviour() {
  command -v cargo-deny >/dev/null || { echo "FAIL: cargo-deny is not installed" >&2; exit 1; }
  [[ -f "${CONFIG}" ]] || { echo "FAIL: ${CONFIG} is missing" >&2; exit 1; }
  local tmp out rc
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN
  if ! cargo-deny --manifest-path "${ROOT}/Cargo.toml" --config "${CONFIG}" check sources >"${tmp}/real.log" 2>&1; then
    sed 's/^/  | /' "${tmp}/real.log" >&2
    echo "FAIL: cargo-deny refused deny-sources.toml on the current graph" >&2
    exit 1
  fi
  grep -v '^allow-git' "${CONFIG}" >"${tmp}/no-allow.toml"
  set +e
  cargo-deny --manifest-path "${ROOT}/Cargo.toml" --config "${tmp}/no-allow.toml" check sources >"${tmp}/no-allow.log" 2>&1
  rc=$?
  set -e
  out="$(grep -c 'source-not-allowed' "${tmp}/no-allow.log" || true)"
  if [[ "${rc}" -eq 0 ]] || ! grep -q 'aya-rs/aya' "${tmp}/no-allow.log" || [[ "${out}" -eq 0 ]]; then
    sed 's/^/  | /' "${tmp}/no-allow.log" >&2
    echo "FAIL: without allow-git the sources check must refuse the Aya git sources (exit ${rc}); its graph may no longer contain assay-ebpf" >&2
    exit 1
  fi
  echo "deny-sources behaviour: current graph passes; without allow-git, ${out} Aya git source(s) refused (exit ${rc})"
}

case "${MODE}" in
  --static) static ;;
  --behaviour) behaviour ;;
  *) echo "usage: $0 [--static|--behaviour]" >&2; exit 2 ;;
esac
