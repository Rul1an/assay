#!/usr/bin/env bash
# Validate all Structurizr workspaces in the repository.
#
# Local default: prefer native structurizr-cli, else Docker with the pinned image.
# CI must set STRUCTURIZR_FORCE_DOCKER=1 so validation always uses the digest-pinned
# container and never a coincidentally installed native CLI. With force-docker,
# missing Docker is a hard failure (no native fallback).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACES=("$ROOT"/docs/architecture/structurizr/*/workspace.dsl)
STRUCTURIZR_CLI_IMAGE="$("${ROOT}/scripts/structurizr-cli-image.sh")"
FORCE_DOCKER="${STRUCTURIZR_FORCE_DOCKER:-0}"

if [[ ${#WORKSPACES[@]} -eq 0 ]]; then
  echo "[structurizr] No workspaces found"
  exit 0
fi

validate_with_cli() {
  local dsl="$1"
  echo "[structurizr] validate: $dsl"
  structurizr-cli validate -workspace "$dsl"
}

validate_with_docker() {
  local dsl="$1"
  local dir
  dir="$(dirname "$dsl")"
  local file
  file="$(basename "$dsl")"
  echo "[structurizr] validate (docker): $dsl"
  docker run --rm -v "$dir:/workspace" "${STRUCTURIZR_CLI_IMAGE}" \
    validate -workspace "/workspace/$file"
}

force_docker_enabled() {
  case "${FORCE_DOCKER}" in
    1|true|TRUE|yes|YES) return 0 ;;
    *) return 1 ;;
  esac
}

ERRORS=0
if force_docker_enabled; then
  if ! command -v docker &>/dev/null; then
    echo "[structurizr] ERROR: STRUCTURIZR_FORCE_DOCKER is set but docker was not found" >&2
    echo "  CI must use the digest-pinned image; native structurizr-cli is not a fallback" >&2
    echo "  Install Docker, or: docker pull ${STRUCTURIZR_CLI_IMAGE}" >&2
    exit 2
  fi
  echo "[structurizr] force-docker: using pinned image ${STRUCTURIZR_CLI_IMAGE}"
  for dsl in "${WORKSPACES[@]}"; do
    validate_with_docker "$dsl" || ERRORS=$((ERRORS + 1))
  done
else
  for dsl in "${WORKSPACES[@]}"; do
    if command -v structurizr-cli &>/dev/null; then
      validate_with_cli "$dsl" || ERRORS=$((ERRORS + 1))
    elif command -v docker &>/dev/null; then
      validate_with_docker "$dsl" || ERRORS=$((ERRORS + 1))
    else
      echo "[structurizr] ERROR: neither structurizr-cli nor docker found"
      echo "  Install: brew install structurizr-cli"
      echo "  Or:      docker pull ${STRUCTURIZR_CLI_IMAGE}"
      exit 2
    fi
  done
fi

if [[ $ERRORS -gt 0 ]]; then
  echo "[structurizr] FAIL: $ERRORS workspace(s) failed validation"
  exit 1
fi

echo "[structurizr] PASS: ${#WORKSPACES[@]} workspace(s) validated"
