#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REV="${BASE_REV:-origin/main}"
PACKAGES=(assay-core assay-evidence assay-registry assay-policy assay-metrics)

if ! git rev-parse --verify --quiet "${BASE_REV}^{commit}" >/dev/null; then
  echo "[api-drift] skip: BASE_REV ${BASE_REV} is not available locally"
  exit 0
fi

ran_any=0

if cargo semver-checks --version >/dev/null 2>&1; then
  ran_any=1
  echo "[api-drift] cargo-semver-checks vs ${BASE_REV}"
  for package in "${PACKAGES[@]}"; do
    echo "[api-drift] semver-checks package=${package}"
    cargo semver-checks check-release -p "${package}" --baseline-rev "${BASE_REV}"
  done
else
  echo "[api-drift] skip cargo-semver-checks: cargo subcommand not installed"
fi

if cargo public-api --version >/dev/null 2>&1; then
  ran_any=1
  echo "[api-drift] cargo-public-api diff vs ${BASE_REV}"
  for package in "${PACKAGES[@]}"; do
    echo "[api-drift] public-api package=${package}"
    if cargo public-api diff --help 2>/dev/null | rg -- '--package|-p' >/dev/null; then
      cargo public-api diff --package "${package}" "${BASE_REV}..HEAD" -sss
    elif cargo public-api --help 2>/dev/null | rg -- '--package|-p' >/dev/null; then
      cargo public-api --package "${package}" diff "${BASE_REV}..HEAD" -sss
    else
      echo "[api-drift] skip package-scoped public-api diff for ${package}: installed cargo-public-api does not advertise --package"
    fi
  done
else
  echo "[api-drift] skip cargo-public-api: cargo subcommand not installed"
fi

if [ "${ran_any}" -eq 0 ]; then
  echo "[api-drift] no optional public API drift tools installed; install cargo-semver-checks and/or cargo-public-api to enable this gate"
fi
