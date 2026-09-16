#!/usr/bin/env bash
# Exercise the actual channel battery from stable, RC, and beta source trees.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# No build, auth state, or Git metadata is needed by these mocked consumers.
for path in scripts infra assay-action .github; do
  cp -R "$ROOT/$path" "$scratch/$path"
done
for version in 6.3.0 6.3.1-rc.1 6.4.0-beta.2; do
  printf '[workspace.package]\nversion = "%s"\n' "$version" > "$scratch/Cargo.toml"
  printf 'channel_workspace=%s\n' "$version"
  (cd "$scratch"; bash scripts/ci/test-release-channel-separation.sh)
done
