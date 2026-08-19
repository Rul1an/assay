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
# CI-4D2 / #2224: split-wave plugins (nextest, hack, semver-checks). Measured against
# crates.io max_stable_version and Rust 1.96 (each tool's MSRV is below that).
export CARGO_NEXTEST_VERSION="0.9.143"
export CARGO_HACK_VERSION="0.6.45"
# cargo-semver-checks is ONE signal, not a source-compatibility proof. Measured on 0.50.0: a public
# generic whose accepted iterator item type narrows from an owned value to a reference passes all
# 223 checks and reports "no semver update required", while a caller passing a `Vec` fails to
# compile with E0271 (#2356). The compensating signal is
# `crates/assay-evidence/tests/public_api_source_compat.rs`, which fails to build on exactly that
# change. Re-measure this note when the pinned version moves; it may stop being true, which would be
# good news.
export CARGO_SEMVER_CHECKS_VERSION="0.50.0"
# CI-4D3 / #2224: optional public-api + mutants. Measured crates.io max_stable_version,
# unyanked. cargo-public-api publishes no rust_version (do not claim formal MSRV; CI Rust
# 1.96 compatibility is integration evidence only). cargo-mutants MSRV 1.88.
export CARGO_PUBLIC_API_VERSION="0.52.0"
export CARGO_MUTANTS_VERSION="27.1.0"

# Where `cargo install` writes binaries: CARGO_INSTALL_ROOT, else CARGO_HOME, else ~/.cargo.
cargo_plugin_bin_path() {
  local name="$1"
  printf '%s\n' "${CARGO_INSTALL_ROOT:-${CARGO_HOME:-${HOME}/.cargo}}/bin/${name}"
}

# Bind the assertion to Cargo's install location. Resolving through PATH lets an unrelated
# binary earlier on PATH satisfy the pin while the install wrote nothing (#2226).
#
# cargo-hack and cargo-mutants require their Cargo subcommand token even when the installed
# binary is invoked directly: `cargo-hack hack --version` and
# `cargo-mutants mutants --version`. Other pinned plugins measured here accept
# `<installed-binary> --version`.
#
# cargo-nextest alone prints a multi-line banner. Measured shape of line 1:
#   cargo-nextest <semver> (<hex> <YYYY-MM-DD>)
# Only that tool may take the first line and strip that documented suffix. Other plugins
# (cargo-audit, cargo-hack, cargo-semver-checks) keep exact full --version equality so a
# parenthetical cannot silently satisfy the pin.
assert_cargo_plugin_version() {
  local name="$1" expected="$2"
  local bin reported
  bin="$(cargo_plugin_bin_path "${name}")"
  if [[ ! -x "${bin}" ]]; then
    echo "${name} missing at install location ${bin}" >&2
    echo "the pin lives in scripts/ci/cargo-plugin-versions.sh and is the only place to change it" >&2
    return 1
  fi
  case "${name}" in
    cargo-hack) reported="$("${bin}" hack --version)" ;;
    cargo-mutants) reported="$("${bin}" mutants --version)" ;;
    *) reported="$("${bin}" --version)" ;;
  esac
  if [[ "${name}" == "cargo-nextest" ]]; then
    reported="${reported%%$'\n'*}"
    # Bash 3.2: =~ + BASH_REMATCH. Require hex hash + ISO date inside the parens.
    if [[ "${reported}" =~ ^cargo-nextest\ ([0-9]+\.[0-9]+\.[0-9]+)\ \(([0-9a-f]+)\ ([0-9]{4}-[0-9]{2}-[0-9]{2})\)$ ]]; then
      reported="cargo-nextest ${BASH_REMATCH[1]}"
    fi
  fi
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
