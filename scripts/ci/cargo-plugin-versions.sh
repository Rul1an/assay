#!/usr/bin/env bash
# Shared cargo-plugin version pins for CI.
#
# Source this file; do not execute it as an installer. Install steps and runtime version
# assertions must both read the variables defined here so a restated literal cannot drift
# (AGENTS.md pinning rule). CI-4D1 owns cargo-audit; later CI-4D2/3 slices extend this file
# with additional CARGO_*_VERSION entries rather than adding another tool-specific installer.
#
# Bash 3.2 compatible (macOS /bin/bash): no associative arrays, no ${var@Q}.
set -euo pipefail

# Exported so a sourcing install step and its version assertion share one value.
export CARGO_AUDIT_VERSION="0.22.2"

# Where `cargo install` writes binaries: CARGO_INSTALL_ROOT, else CARGO_HOME, else ~/.cargo.
cargo_plugin_bin_path() {
  local name="$1"
  printf '%s\n' "${CARGO_INSTALL_ROOT:-${CARGO_HOME:-${HOME}/.cargo}}/bin/${name}"
}

# Bind the assertion to Cargo's install location. Resolving through PATH lets an unrelated
# binary earlier on PATH satisfy the pin while the install wrote nothing (#2226).
assert_cargo_plugin_version() {
  local name="$1" expected="$2"
  local bin reported
  bin="$(cargo_plugin_bin_path "${name}")"
  if [[ ! -x "${bin}" ]]; then
    echo "${name} missing at install location ${bin}" >&2
    echo "the pin lives in scripts/ci/cargo-plugin-versions.sh and is the only place to change it" >&2
    return 1
  fi
  reported="$("${bin}" --version)"
  echo "${name} resolved: ${reported} at ${bin}"
  if [[ "${reported}" != "${name} ${expected}" ]]; then
    echo "${name} reports '${reported}', which is not the pinned ${expected}" >&2
    echo "the pin lives in scripts/ci/cargo-plugin-versions.sh and is the only place to change it" >&2
    return 1
  fi
}

# When sourced, define pins and helpers only. When executed, refuse — this is not an installer.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  echo "source scripts/ci/cargo-plugin-versions.sh; it is a version source, not an installer" >&2
  exit 1
fi
