#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-Rul1an/assay}"
VM_NAME="${VM_NAME:-assay-bpf-runner}"
HARNESS_DIR="${HARNESS_DIR:-../Assay-Harness}"
CHECK_VM="${CHECK_VM:-1}"
EXPECTED_RELEASE="${EXPECTED_RELEASE:-}"
REQUIRED_RUBY_VERSION="3.3.12"
REQUIRED_PSYCH_VERSION="5.1.2"

failures=0

note() {
  printf '%s\n' "$*"
}

fail() {
  failures=$((failures + 1))
  note "FAIL: $*"
}

if ! command -v ruby >/dev/null 2>&1; then
  echo "release version-line parser requires Ruby ${REQUIRED_RUBY_VERSION} with Psych ${REQUIRED_PSYCH_VERSION}" >&2
  exit 1
fi
parser_versions="$(ruby -ryaml -e 'print [RUBY_VERSION, Psych::VERSION].join(" ")')"
if [[ "$parser_versions" != "${REQUIRED_RUBY_VERSION} ${REQUIRED_PSYCH_VERSION}" ]]; then
  echo "release version-line parser requires Ruby ${REQUIRED_RUBY_VERSION} with Psych ${REQUIRED_PSYCH_VERSION}; found ${parser_versions}" >&2
  exit 1
fi

latest_tag() {
  local tag
  tag=$(
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" |
      sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
      head -n 1
  )
  if [[ ! "$tag" =~ ^v[0-9]+[.][0-9]+[.][0-9]+$ ]]; then
    echo "latest Assay release is not a stable software tag: $tag" >&2
    return 1
  fi
  printf '%s\n' "$tag"
}

is_stable_tag() {
  [[ "$1" =~ ^v[0-9]+[.][0-9]+[.][0-9]+$ ]]
}

workspace_version() {
  awk '
    $0 == "[workspace.package]" { in_workspace_package = 1; next }
    /^\[/ && $0 != "[workspace.package]" { in_workspace_package = 0 }
    in_workspace_package && $1 == "version" {
      gsub(/"/, "", $3)
      print $3
      exit
    }
  ' Cargo.toml
}

harness_version() {
  local workflow="${HARNESS_DIR}/.github/workflows/harness-ci.yml"
  if [[ ! -f "$workflow" ]]; then
    return 0
  fi

  # shellcheck disable=SC2016 # Ruby reads the YAML structure, not shell variables.
  ruby -ryaml -e '
    def reject_duplicate_keys(node, scanner, path = [])
      case node
      when Psych::Nodes::Stream, Psych::Nodes::Document
        node.children.each { |child| reject_duplicate_keys(child, scanner, path) }
      when Psych::Nodes::Mapping
        seen = {}
        node.children.each_slice(2) do |key_node, value_node|
          abort("complex YAML mapping keys are unsupported") unless
            key_node.is_a?(Psych::Nodes::Scalar)
          abort("explicitly tagged YAML mapping keys are unsupported") if
            key_node.tag
          raw_key = key_node.value
          semantic_key = key_node.plain ? scanner.tokenize(raw_key) : raw_key
          fingerprint = [semantic_key.class.name, semantic_key]
          abort("duplicate YAML key at #{(path + [raw_key]).join(".")}") if
            seen[fingerprint]
          seen[fingerprint] = true
          reject_duplicate_keys(value_node, scanner, path + [raw_key])
        end
      when Psych::Nodes::Sequence
        node.children.each_with_index do |child, index|
          reject_duplicate_keys(child, scanner, path + [index.to_s])
        end
      end
    end

    loader = Psych::ClassLoader::Restricted.new([], [])
    scanner = Psych::ScalarScanner.new(loader)
    reject_duplicate_keys(Psych.parse_file(ARGV.fetch(0)), scanner)
    document = YAML.safe_load_file(ARGV.fetch(0), aliases: false)
    triggers = document["on"] || document[true]
    value = triggers
      &.dig("workflow_dispatch", "inputs", "assay_version", "default")
    abort("missing workflow_dispatch.inputs.assay_version.default") unless
      value.is_a?(String) && !value.empty?
    puts value
  ' "$workflow"
}

vm_assay_version() {
  if ! command -v multipass >/dev/null 2>&1; then
    return 0
  fi

  # shellcheck disable=SC2016 # awk expansion must happen inside the VM shell.
  multipass exec "$VM_NAME" -- sudo -u github-runner bash -lc \
    'assay --version 2>/dev/null | awk "{print \$2}"' 2>/dev/null || true
}

latest=""
if ! latest="$(latest_tag)"; then
  fail "latest ${REPO} release is not a stable software tag"
elif [[ -z "$latest" ]]; then
  fail "could not resolve latest ${REPO} release"
else
  note "latest_release=${latest}"
fi

release_target="$latest"
if [[ -n "$EXPECTED_RELEASE" ]]; then
  if ! is_stable_tag "$EXPECTED_RELEASE"; then
    fail "expected release is not a stable software tag: ${EXPECTED_RELEASE}"
  else
    release_target="$EXPECTED_RELEASE"
  fi
fi
if [[ -n "$release_target" ]]; then
  note "release_target=${release_target}"
fi

workspace="$(workspace_version)"
if [[ -z "$workspace" ]]; then
  fail "could not read workspace.package.version from Cargo.toml"
else
  note "workspace_version=${workspace}"
fi

harness="$(harness_version)"
if [[ -z "$harness" ]]; then
  fail "could not read Harness CI assay_version default from ${HARNESS_DIR}"
else
  note "harness_compatibility_assay_version=${harness}"
fi

vm_version=""
if [[ "$CHECK_VM" == "1" ]]; then
  vm_version="$(vm_assay_version)"
  if [[ -z "$vm_version" ]]; then
    fail "could not read assay version from Multipass VM ${VM_NAME}"
  else
    note "vm_assay_version=${vm_version}"
  fi
else
  note "vm_assay_version=skipped"
fi

if [[ -n "$release_target" && -n "$workspace" && "v${workspace}" != "$release_target" ]]; then
  fail "workspace version v${workspace} does not match release target ${release_target}"
fi

if [[ -n "$harness" ]] && ! is_stable_tag "$harness"; then
  fail "Harness compatibility default is not a stable software tag: ${harness}"
fi

if [[ "$CHECK_VM" == "1" && -n "$latest" && -n "$vm_version" && "v${vm_version}" != "$latest" ]]; then
  fail "VM assay version v${vm_version} does not match latest release ${latest}"
fi

if [[ "$failures" -gt 0 ]]; then
  note "version_line_status=failed"
  exit 1
fi

note "version_line_status=ok"
