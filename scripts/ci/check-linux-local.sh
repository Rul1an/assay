#!/bin/bash
set -e

# check-linux-local.sh
# Runs cargo check/clippy for Linux target using Docker to catch OS-specific errors locally.

IMAGE="rust:latest"

if ! command -v docker &> /dev/null; then
    echo "Error: Docker is required to run Linux checks locally."
    exit 1
fi

echo "🐳 Running Linux checks in Docker ($IMAGE)..."

# Mount current dir to /volume
docker run --rm \
    -v "$(pwd)":/volume \
    -w /volume \
    -e RUSTFLAGS="-D warnings" \
    "$IMAGE" \
    bash -c "cargo check --workspace --all-targets && cargo clippy --workspace --all-targets"

echo "✅ Linux checks passed!"
