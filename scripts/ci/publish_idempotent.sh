#!/bin/bash
set -euo pipefail

# List of crates in topological order (dependencies first)
CRATES=(
  "assay-common"
  "assay-policy"
  "assay-core"
  "assay-metrics"
  "assay-monitor"
  "assay-mcp-server"
  "assay-cli"
)

echo "📦 Starting Idempotent Publisher..."

for crate in "${CRATES[@]}"; do
  echo "Checking $crate..."

  # Get local version from Cargo.toml
  # We assume the structure crates/<name>/Cargo.toml exists
  manifest="crates/$crate/Cargo.toml"
  if [ ! -f "$manifest" ]; then
    echo "ERROR: Manifest $manifest not found!"
    exit 1
  fi

  version=$(grep '^version =' "$manifest" | head -n1 | cut -d '"' -f 2)
  echo "Local version: $version"

  # Check crates.io API
  # Returns 200 if found, 404/other if not.
  # We use HTTP status checking.
  url="https://crates.io/api/v1/crates/${crate}/${version}"
  http_code=$(curl -s -o /dev/null -w "%{http_code}" "$url")

  if [ "$http_code" == "200" ]; then
    echo "✅ ${crate}@${version} already on crates.io. Skipping."
  else
    echo "🚀 Publishing ${crate}@${version}..."
    # We must allow a small sleep for index propagation between dependencies
    cargo publish --package "$crate" --verbose
    echo "Sleeping 45s for index propagation..."
    sleep 45
  fi
  echo "---------------------------------------------------"
done

echo "🎉 All crates processed."
