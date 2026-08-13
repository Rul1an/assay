#!/usr/bin/env bash
# Mutate send-attach, rebuild, run the attach-disabled matrix, restore the real binary.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

restore() {
  cp "$bak" "$REAL_SRC_PATH"
  local tmpf="${REAL_BIN_PATH}.tmp"
  cp "$binbak" "$tmpf"
  cmp -s "$tmpf" "$binbak"
  mv -f "$tmpf" "$REAL_BIN_PATH"
  test -f "$REAL_BIN_PATH"
  cmp -s "$REAL_BIN_PATH" "$binbak"
  rm -f "$bak" "$binbak" "$tmpf"
}

if [[ "${1:-}" == restore-selftest ]]; then
  t=$(mktemp -d)
  trap 'rm -rf "$t"' EXIT
  REAL_SRC_PATH="$t/loader.rs"
  REAL_BIN_PATH="$t/assay"
  printf 'orig-src\n' >"$REAL_SRC_PATH"
  printf 'orig-bin\n' >"$REAL_BIN_PATH"
  bak="$t/loader.bak"
  binbak="$t/assay.bak"
  cp "$REAL_SRC_PATH" "$bak"
  cp "$REAL_BIN_PATH" "$binbak"
  readonly -f restore
  bin="$t/decoy-target"
  src="$t/decoy-src"
  printf 'decoy-bin\n' >"$bin"
  printf 'decoy-src\n' >"$src"
  printf 'mutated-bin\n' >"$REAL_BIN_PATH"
  printf 'mutated-src\n' >"$REAL_SRC_PATH"
  readonly REAL_SRC_PATH REAL_BIN_PATH bak binbak
  if (REAL_BIN_PATH="$t/rebind-decoy") 2>/dev/null; then
    echo "FAIL: REAL_BIN_PATH rebind after capture succeeded" >&2
    exit 1
  fi
  restore
  grep -qx 'orig-bin' "$REAL_BIN_PATH" || { echo "FAIL: restore missed original binary" >&2; exit 1; }
  grep -qx 'orig-src' "$REAL_SRC_PATH" || { echo "FAIL: restore missed original source" >&2; exit 1; }
  grep -qx 'decoy-bin' "$bin" || { echo "FAIL: decoy binary mutated" >&2; exit 1; }
  grep -qx 'decoy-src' "$src" || { echo "FAIL: decoy source mutated" >&2; exit 1; }
  echo "ok: restore-selftest"
  exit 0
fi

REAL_SRC_PATH="$ROOT/crates/assay-monitor/src/loader.rs"
REAL_BIN_PATH="$ROOT/target/release/assay"
: "${RUNNER_TEMP:?RUNNER_TEMP required}"
bak="$RUNNER_TEMP/loader.rs.s1b.bak"
binbak="$RUNNER_TEMP/assay.s1b.bin.bak"
cp "$REAL_SRC_PATH" "$bak"
cp "$REAL_BIN_PATH" "$binbak"
readonly REAL_SRC_PATH REAL_BIN_PATH bak binbak
readonly -f restore
trap restore EXIT
trap 'exit 143' TERM
trap 'exit 130' INT
bash scripts/ci/run-send-syscall-matrix.sh disable-send-attach
cargo build -p assay-cli --release
python3 -c 'import pathlib,sys; sys.exit(0 if b"s1b-cell7-disabled" in pathlib.Path(sys.argv[1]).read_bytes() else 1)' \
  "$REAL_BIN_PATH" || {
  echo "FAIL: ASSAY_BIN is not the mutated rebuild (missing s1b-cell7-disabled)" >&2
  exit 1
}
sudo -E env HARNESS_BIN="$RUNNER_TEMP/s1b-harness" WORKDIR="$RUNNER_TEMP/s1b-disabled" \
  bash scripts/ci/run-send-syscall-matrix.sh attach-disabled
