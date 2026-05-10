#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

echo "[review] dependency hygiene guards"
if rg -q '(^|[[:space:]])sha256[[:space:]]*=' crates/assay-core/Cargo.toml Cargo.toml; then
  echo "FAIL: direct sha256 crate dependency should stay removed; use workspace sha2 helpers instead" >&2
  exit 1
fi
if rg -q '^name = "sha256"$|"sha256"' Cargo.lock; then
  echo "FAIL: Cargo.lock should not contain the sha256 crate after PR4 cleanup" >&2
  exit 1
fi
if cargo tree -p assay-core -i sha256 >/tmp/assay-pr4-sha256-tree.txt 2>&1; then
  cat /tmp/assay-pr4-sha256-tree.txt >&2
  echo "FAIL: sha256 crate is still reachable from assay-core" >&2
  exit 1
fi

if rg -q 'RUSTSEC-2026-0097|github\.com/aya-rs/aya' deny.toml; then
  echo "FAIL: stale deny.toml advisory/source exceptions should stay removed" >&2
  exit 1
fi

if ! rg -q 'serde_yaml' docs/architecture/PLAN-DEPENDENCY-PERF-HYGIENE-PR4-2026q2.md; then
  echo "FAIL: PR4 must keep an explicit serde_yaml retirement note without migrating it" >&2
  exit 1
fi

echo "[review] cargo deny scope"
cargo deny check advisories bans sources

echo "[review] format/check/clippy"
cargo fmt --check
cargo check -p assay-core
cargo clippy -p assay-core --all-targets -- -D warnings

echo "[review] diff hygiene"
git diff --check
