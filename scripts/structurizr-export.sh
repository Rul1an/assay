#!/usr/bin/env bash
# Export Structurizr workspaces to Mermaid diagrams.
#
# Outputs to docs/architecture/structurizr/<name>/export/
# These are text-based and safe to commit.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORKSPACES=("$ROOT"/docs/architecture/structurizr/*/workspace.dsl)

if [[ ${#WORKSPACES[@]} -eq 0 ]]; then
  echo "[structurizr] No workspaces found"
  exit 0
fi

export_with_cli() {
  local dsl="$1"
  local outdir="$2"
  mkdir -p "$outdir"
  echo "[structurizr] export: $dsl → $outdir"
  structurizr-cli export -workspace "$dsl" -format mermaid -output "$outdir"
}

export_with_docker() {
  local dsl="$1"
  local dir
  dir="$(dirname "$dsl")"
  local file
  file="$(basename "$dsl")"
  local outdir="$2"
  local rel_out
  rel_out="$(realpath --relative-to="$dir" "$outdir")"
  mkdir -p "$outdir"
  echo "[structurizr] export (docker): $dsl → $outdir"
  docker run --rm -v "$dir:/workspace" structurizr/cli:latest \
    export -workspace "/workspace/$file" -format mermaid -output "/workspace/$rel_out"
}

for dsl in "${WORKSPACES[@]}"; do
  ws_dir="$(dirname "$dsl")"
  outdir="$ws_dir/export"

  if command -v structurizr-cli &>/dev/null; then
    export_with_cli "$dsl" "$outdir"
  elif command -v docker &>/dev/null; then
    export_with_docker "$dsl" "$outdir"
  else
    echo "[structurizr] ERROR: neither structurizr-cli nor docker found"
    exit 2
  fi
done

echo "[structurizr] Export complete"
