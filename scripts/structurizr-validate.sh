#!/usr/bin/env bash
# Validate all Structurizr workspaces in the repository.
#
# Requires: structurizr-cli (brew install structurizr-cli) or Docker.
# CI uses the Docker path; local dev can use either.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACES=("$ROOT"/docs/architecture/structurizr/*/workspace.dsl)

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
  docker run --rm -v "$dir:/workspace" structurizr/cli:latest \
    validate -workspace "/workspace/$file"
}

ERRORS=0
for dsl in "${WORKSPACES[@]}"; do
  if command -v structurizr-cli &>/dev/null; then
    validate_with_cli "$dsl" || ERRORS=$((ERRORS + 1))
  elif command -v docker &>/dev/null; then
    validate_with_docker "$dsl" || ERRORS=$((ERRORS + 1))
  else
    echo "[structurizr] ERROR: neither structurizr-cli nor docker found"
    echo "  Install: brew install structurizr-cli"
    echo "  Or:      docker pull structurizr/cli:latest"
    exit 2
  fi
done

if [[ $ERRORS -gt 0 ]]; then
  echo "[structurizr] FAIL: $ERRORS workspace(s) failed validation"
  exit 1
fi

echo "[structurizr] PASS: ${#WORKSPACES[@]} workspace(s) validated"
