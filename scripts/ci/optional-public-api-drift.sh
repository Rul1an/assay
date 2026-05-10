#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

BASE_REV="${BASE_REV:-origin/main}"
PACKAGES=(assay-core assay-evidence assay-registry assay-policy assay-metrics)
INSTALL_TOOLS="${ASSAY_INSTALL_API_DRIFT_TOOLS:-0}"
FAIL_ON_MISSING_BASE="${ASSAY_API_DRIFT_FAIL_ON_MISSING_BASE:-${GITHUB_ACTIONS:-0}}"

if [ -n "${ASSAY_API_DRIFT_PACKAGES:-}" ]; then
  # shellcheck disable=SC2206
  PACKAGES=(${ASSAY_API_DRIFT_PACKAGES})
fi

ensure_base_rev() {
  if git rev-parse --verify --quiet "${BASE_REV}^{commit}" >/dev/null; then
    return 0
  fi

  echo "[api-drift] BASE_REV ${BASE_REV} is not available locally; attempting fetch"
  if [[ "${BASE_REV}" == origin/* ]]; then
    local remote_branch="${BASE_REV#origin/}"
    git fetch --no-tags origin "${remote_branch}:refs/remotes/origin/${remote_branch}" || true
  else
    git fetch --no-tags origin "${BASE_REV}" || git fetch --no-tags origin "${BASE_REV}:refs/temp/api-drift-baseline" || true
  fi

  git rev-parse --verify --quiet "${BASE_REV}^{commit}" >/dev/null
}

if ! ensure_base_rev; then
  if [ "${FAIL_ON_MISSING_BASE}" = "1" ] || [ "${FAIL_ON_MISSING_BASE}" = "true" ]; then
    echo "[api-drift] FAIL: BASE_REV ${BASE_REV} is not available after fetch" >&2
    exit 1
  fi
  echo "[api-drift] skip: BASE_REV ${BASE_REV} is not available locally"
  exit 0
fi

ensure_cargo_subcommand() {
  local subcommand="$1"
  local crate="$2"
  if cargo "${subcommand}" --version >/dev/null 2>&1; then
    return 0
  fi
  if [ "${INSTALL_TOOLS}" = "1" ]; then
    echo "[api-drift] installing ${crate}"
    cargo install --locked "${crate}"
    cargo "${subcommand}" --version >/dev/null
    return 0
  fi
  return 1
}

ran_any=0

if ensure_cargo_subcommand semver-checks cargo-semver-checks; then
  ran_any=1
  echo "[api-drift] cargo-semver-checks vs ${BASE_REV}"
  for package in "${PACKAGES[@]}"; do
    echo "[api-drift] semver-checks package=${package}"
    cargo semver-checks check-release -p "${package}" --baseline-rev "${BASE_REV}"
  done
else
  echo "[api-drift] skip cargo-semver-checks: cargo subcommand not installed"
fi

if ensure_cargo_subcommand public-api cargo-public-api; then
  ran_any=1
  echo "[api-drift] cargo-public-api diff vs ${BASE_REV}"
  for package in "${PACKAGES[@]}"; do
    echo "[api-drift] public-api package=${package}"
    if cargo public-api diff --help 2>/dev/null | grep -qE -- '--package|-p'; then
      cargo public-api diff --package "${package}" "${BASE_REV}..HEAD" -sss
    elif cargo public-api --help 2>/dev/null | grep -qE -- '--package|-p'; then
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
