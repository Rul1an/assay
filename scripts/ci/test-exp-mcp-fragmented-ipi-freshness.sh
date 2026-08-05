#!/usr/bin/env bash
# Asserts the freshness invariant of the fragmented-IPI harness: every wrapper builds, into the
# directory the driver reads, the packages that its own configuration will actually execute.
#
# This exists because the invariant was previously carried only by comments, and comments drift.
# Every case below corresponds to a defect found in review of PR #1989: gating the build on
# RUN_LIVE left the live path unguarded even though the rerun docs point ASSAY_CMD at the local
# binary; an unpinned target directory let CARGO_TARGET_DIR send the build somewhere the driver
# never opens; and an unconditional build silently disabled SKIP_CARGO_BUILD.
#
# Cases A-H drive the wrappers through a transparent cargo shim that records argv and then execs
# the real cargo, so they assert the build decision. They do not require the run itself to
# succeed -- the build has already happened by then -- so wrapper exit status is ignored.
# Case I is the end-to-end property: a stale binary is actually rebuilt.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXP="$ROOT/scripts/ci/exp-mcp-fragmented-ipi"
REAL_CARGO="$(command -v cargo)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

mkdir -p "$TMP/bin"
cat > "$TMP/bin/cargo" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$*" >> "\$CARGO_SHIM_LOG"
exec "$REAL_CARGO" "\$@"
EOF
chmod +x "$TMP/bin/cargo"

# Cases A-H measure the build decision, which happens before the driver is ever invoked, so the
# driver is stubbed out. This is not a shortcut: these cases must run against BROKEN wrappers to
# be worth anything, and a real driver handed a non-MCP host (RUN_LIVE=1 with a stub
# MCP_HOST_CMD) blocks forever on a response that never comes. A test that can hang is no use as
# a regression gate. Case I drives the real thing.
cat > "$TMP/bin/python3" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$TMP/bin/python3"

FAILURES=0
CASE=""

# Run a wrapper with the shim active and capture what it asked cargo to build.
# usage: record <case-label> <script> [args...]   (env vars come from the caller)
record() {
  CASE="$1"; shift
  CARGO_SHIM_LOG="$TMP/cargo.log"
  export CARGO_SHIM_LOG
  : > "$CARGO_SHIM_LOG"
  PATH="$TMP/bin:$PATH" "$@" >"$TMP/out.log" 2>&1 || true
}

ok()   { printf '  ok    %s\n' "$1"; }
bad()  { printf '  FAIL  %s\n' "$1"; FAILURES=$((FAILURES + 1)); }

# Assert the recorded cargo invocation does / does not mention a string. Written as if-then-else
# rather than `A && B || C`: with the latter a failing `bad` would fall through to `ok` and a
# real failure would print as a pass.
built() {
  if grep -q -- "$1" "$TMP/cargo.log"; then
    ok "$CASE: builds $1"
  else
    bad "$CASE: expected to build $1; cargo log: $(cat "$TMP/cargo.log")"
  fi
}
not_built() {
  if grep -q -- "$1" "$TMP/cargo.log"; then
    bad "$CASE: must NOT build $1; cargo log: $(cat "$TMP/cargo.log")"
  else
    ok "$CASE: does not build $1"
  fi
}
no_cargo() {
  if [[ -s "$TMP/cargo.log" ]]; then
    bad "$CASE: expected no cargo call; got: $(cat "$TMP/cargo.log")"
  else
    ok "$CASE: no cargo call"
  fi
}

echo "[freshness] A: baseline RUN_LIVE=0 builds only assay-cli"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 record A bash "$EXP/run_baseline.sh" "$TMP/a"
built "-p assay-cli"
not_built "assay-mcp-server"
built "--target-dir $ROOT/target"

echo "[freshness] B: baseline RUN_LIVE=1 still builds assay-cli (ASSAY_CMD is the local binary)"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=1 MCP_HOST_CMD=true ASSAY_CMD="$ROOT/target/debug/assay" \
  record B bash "$EXP/run_baseline.sh" "$TMP/b"
built "-p assay-cli"

echo "[freshness] C: protected with sidecar builds both packages"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SEQUENCE_SIDECAR=1 record C bash "$EXP/run_protected.sh" "$TMP/c"
built "-p assay-cli"
built "-p assay-mcp-server"

echo "[freshness] D: protected without sidecar does not build the guard"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SEQUENCE_SIDECAR=0 record D bash "$EXP/run_protected.sh" "$TMP/d"
built "-p assay-cli"
not_built "assay-mcp-server"

echo "[freshness] E: protected RUN_LIVE=1 + sidecar builds BOTH, not just the guard"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=1 SEQUENCE_SIDECAR=1 MCP_HOST_CMD=true \
  ASSAY_CMD="$ROOT/target/debug/assay" record E bash "$EXP/run_protected.sh" "$TMP/e"
built "-p assay-cli"
built "-p assay-mcp-server"

echo "[freshness] F: SKIP_CARGO_BUILD=1 is honoured"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SKIP_CARGO_BUILD=1 record F bash "$EXP/run_baseline.sh" "$TMP/f"
no_cargo

echo "[freshness] G: CARGO_TARGET_DIR cannot divert the build away from where the driver reads"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 CARGO_TARGET_DIR="$TMP/elsewhere" \
  record G bash "$EXP/run_baseline.sh" "$TMP/g"
built "--target-dir $ROOT/target"

echo "[freshness] H: cross-session rejects an out-of-range RUN_LIVE instead of skipping the build"
# Assert the rejection MESSAGE, not just the exit code. Without validation this script runs on
# and the driver dies of the bad value anyway -- also with status 2 -- so an exit-code-only
# assertion passes against the very defect it is meant to catch.
H_STATUS=0
RUN_LIVE=2 MODE=wrap_only OUT_DIR="$TMP/h" \
  bash "$EXP/cross_session/run_cross_session_decay.sh" >"$TMP/h.log" 2>&1 || H_STATUS=$?
if [[ "$H_STATUS" -eq 2 ]] && grep -q "FAIL: RUN_LIVE must be 0 or 1" "$TMP/h.log"; then
  ok "H: RUN_LIVE=2 rejected up front with exit 2"
else
  bad "H: RUN_LIVE=2 must be rejected by validation (exit 2 + explicit message), got exit $H_STATUS: $(tail -1 "$TMP/h.log")"
fi

echo "[freshness] I: a stale binary is actually rebuilt (end-to-end, real cargo and real driver)"
"$REAL_CARGO" build -q --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target" -p assay-cli
touch "$ROOT/crates/assay-cli/src/main.rs"
BEFORE="$(stat -f %m "$ROOT/target/debug/assay" 2>/dev/null || stat -c %Y "$ROOT/target/debug/assay")"
sleep 1
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 bash "$EXP/run_baseline.sh" "$TMP/i" >/dev/null 2>&1 || true
AFTER="$(stat -f %m "$ROOT/target/debug/assay" 2>/dev/null || stat -c %Y "$ROOT/target/debug/assay")"
if [[ "$BEFORE" != "$AFTER" ]]; then
  ok "I: stale binary rebuilt ($BEFORE -> $AFTER)"
else
  bad "I: binary NOT rebuilt; an existence check would have passed here"
fi

echo
if (( FAILURES > 0 )); then
  echo "[freshness] FAILED: $FAILURES assertion(s)"
  exit 1
fi
echo "[freshness] all assertions passed"
