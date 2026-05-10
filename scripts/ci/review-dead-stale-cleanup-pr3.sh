#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."

echo "[review] stale/dead cleanup guard"
if rg -q "NetworkAnalyzer|ProcNetStats|fanout_warn|fanout_deny|port_scan_threshold" \
  crates/assay-cli/src/cli/commands/heuristics.rs; then
  echo "FAIL: stateful NetworkAnalyzer fanout path should stay removed" >&2
  exit 1
fi
if rg -q "current_config" crates/assay-cli/src/cli/commands/config_path.rs; then
  echo "FAIL: stale config_path current_config parsing should stay removed" >&2
  exit 1
fi
if rg -q '#!\[allow\(dead_code\)\]' \
  crates/assay-cli/src/cli/commands/heuristics.rs \
  crates/assay-cli/src/cli/commands/coverage/schema.rs \
  crates/assay-cli/src/cli/commands/sim/soak/schema.rs; then
  echo "FAIL: module-wide dead_code suppressions should stay removed from PR3 targets" >&2
  exit 1
fi

echo "[review] format/check/clippy"
cargo fmt --check
cargo check -p assay-cli
cargo clippy -p assay-cli --all-targets -- -D warnings

echo "[review] focused behavior tests"
cargo test -p assay-cli --bin assay cli::commands::heuristics::tests
cargo test -p assay-cli --bin assay cli::commands::coverage::schema::tests
cargo test -p assay-cli --bin assay cli::commands::sim::soak

echo "[review] diff hygiene"
git diff --check
