#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-Rul1an/assay}"
VM_NAME="${VM_NAME:-assay-bpf-runner}"
HARNESS_DIR="${HARNESS_DIR:-../Assay-Harness}"
CHECK_VM="${CHECK_VM:-1}"

failures=0

note() {
  printf '%s\n' "$*"
}

fail() {
  failures=$((failures + 1))
  note "FAIL: $*"
}

latest_tag() {
  curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" |
    sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -n 1
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

  awk '
    $1 == "default:" {
      value = $2
      gsub(/"/, "", value)
      print value
      exit
    }
  ' "$workflow"
}

vm_assay_version() {
  if ! command -v multipass >/dev/null 2>&1; then
    return 0
  fi

  multipass exec "$VM_NAME" -- sudo -u github-runner bash -lc \
    'assay --version 2>/dev/null | awk "{print \$2}"' 2>/dev/null || true
}

latest="$(latest_tag)"
if [[ -z "$latest" ]]; then
  fail "could not resolve latest ${REPO} release"
else
  note "latest_release=${latest}"
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
  note "harness_assay_version=${harness}"
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

if [[ -n "$latest" && -n "$workspace" && "v${workspace}" != "$latest" ]]; then
  fail "workspace version v${workspace} does not match latest release ${latest}"
fi

if [[ -n "$latest" && -n "$harness" && "$harness" != "$latest" ]]; then
  fail "Harness compatibility default ${harness} does not match latest release ${latest}"
fi

if [[ "$CHECK_VM" == "1" && -n "$latest" && -n "$vm_version" && "v${vm_version}" != "$latest" ]]; then
  fail "VM assay version v${vm_version} does not match latest release ${latest}"
fi

if [[ "$failures" -gt 0 ]]; then
  note "version_line_status=failed"
  exit 1
fi

note "version_line_status=ok"
