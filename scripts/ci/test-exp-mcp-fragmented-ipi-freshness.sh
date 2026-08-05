#!/usr/bin/env bash
# Asserts the freshness invariant of the fragmented-IPI harness: every wrapper builds, into the
# directory the driver reads, the packages that its own configuration will actually execute.
#
# The invariant has two halves and both are asserted here, because checking only the first is the
# same mistake this harness exists to prevent:
#   1. the wrapper asks cargo to build the right packages into the right directory (cases A-K)
#   2. that directory is the one the driver actually opens (case L)
#
# Cases A-K drive the wrappers through a transparent cargo shim that records argv and then execs
# the real cargo. The driver is stubbed for them: they must run against BROKEN wrappers to be
# worth anything, and a real driver handed a non-MCP host blocks forever on a response that never
# comes. They assert the wrapper's exit status too -- recording what cargo was ASKED to do while
# ignoring whether it succeeded would be intent, not result.
# Case M is the end-to-end property: a stale binary is actually rebuilt.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EXP="$ROOT/scripts/ci/exp-mcp-fragmented-ipi"
REAL_CARGO="$(command -v cargo)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

# Record one argument per line so assertions can match whole tokens. A joined command line has to
# be matched by substring, and substrings both false-pass (`--target-dir $ROOT/target` matching
# `$ROOT/target/x`) and false-fail (`assay-mcp-server` matching --manifest-path in a worktree
# named for that crate).
mkdir -p "$TMP/bin"
cat > "$TMP/bin/cargo" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$@" >> "\$CARGO_SHIM_LOG"
exec "$REAL_CARGO" "\$@"
EOF
chmod +x "$TMP/bin/cargo"

cat > "$TMP/bin/python3" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$TMP/bin/python3"

# GNU first: on BSD `stat -c` fails cleanly, whereas GNU `stat -f` means --file-system and prints
# filesystem stats to stdout before failing on the format. That output changes with any disk
# activity, so a BSD-first fallback compares block counts and reports "rebuilt" for a build that
# never happened.
mtime_of() { stat -c %Y "$1" 2>/dev/null || stat -f %m "$1"; }

FAILURES=0
CASE=""
LAST_STATUS=0

ok()  { printf '  ok    %s\n' "$1"; }
bad() { printf '  FAIL  %s\n' "$1"; FAILURES=$((FAILURES + 1)); }

# usage: record <case-label> <script> [args...]   (env comes from the caller)
record() {
  CASE="$1"; shift
  CARGO_SHIM_LOG="$TMP/cargo.log"
  export CARGO_SHIM_LOG
  : > "$CARGO_SHIM_LOG"
  LAST_STATUS=0
  PATH="$TMP/bin:$PATH" "$@" >"$TMP/out.log" 2>&1 || LAST_STATUS=$?
}

arg_present() { grep -Fxq -- "$1" "$TMP/cargo.log"; }

built() {
  if arg_present "$1"; then ok "$CASE: builds $1"
  else bad "$CASE: expected cargo arg '$1'; got: $(tr '\n' ' ' < "$TMP/cargo.log")"; fi
}
not_built() {
  if arg_present "$1"; then bad "$CASE: must NOT build $1; got: $(tr '\n' ' ' < "$TMP/cargo.log")"
  else ok "$CASE: does not build $1"; fi
}
no_cargo() {
  if [[ -s "$TMP/cargo.log" ]]; then bad "$CASE: expected no cargo call; got: $(tr '\n' ' ' < "$TMP/cargo.log")"
  else ok "$CASE: no cargo call"; fi
}
# A wrapper that asked for the right build but died doing it must not count as a pass.
succeeded() {
  if [[ "$LAST_STATUS" -eq 0 ]]; then ok "$CASE: wrapper exited 0"
  else bad "$CASE: wrapper exited $LAST_STATUS: $(tail -2 "$TMP/out.log" | tr '\n' ' ')"; fi
}
target_pinned() { built "--target-dir"; built "$ROOT/target"; }

echo "[freshness] A: baseline RUN_LIVE=0 builds only assay-cli, into the pinned dir"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 record A bash "$EXP/run_baseline.sh" "$TMP/a"
succeeded; built "assay-cli"; not_built "assay-mcp-server"; target_pinned

echo "[freshness] B: baseline RUN_LIVE=1 still builds (ASSAY_CMD is the local binary)"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=1 MCP_HOST_CMD=true ASSAY_CMD="$ROOT/target/debug/assay" \
  record B bash "$EXP/run_baseline.sh" "$TMP/b"
built "assay-cli"; target_pinned

echo "[freshness] C: protected with sidecar builds both, into the pinned dir"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SEQUENCE_SIDECAR=1 record C bash "$EXP/run_protected.sh" "$TMP/c"
succeeded; built "assay-cli"; built "assay-mcp-server"; target_pinned

echo "[freshness] D: protected without sidecar does not build the guard"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SEQUENCE_SIDECAR=0 record D bash "$EXP/run_protected.sh" "$TMP/d"
succeeded; built "assay-cli"; not_built "assay-mcp-server"; target_pinned

echo "[freshness] E: protected RUN_LIVE=1 + sidecar builds BOTH, not just the guard"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=1 SEQUENCE_SIDECAR=1 MCP_HOST_CMD=true \
  ASSAY_CMD="$ROOT/target/debug/assay" record E bash "$EXP/run_protected.sh" "$TMP/e"
built "assay-cli"; built "assay-mcp-server"

echo "[freshness] F: SKIP_CARGO_BUILD is honoured by every wrapper"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SKIP_CARGO_BUILD=1 record F1 bash "$EXP/run_baseline.sh" "$TMP/f1"
no_cargo
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 SKIP_CARGO_BUILD=1 record F2 bash "$EXP/run_protected.sh" "$TMP/f2"
no_cargo
MODE=wrap_only OUT_DIR="$TMP/f3" SKIP_CARGO_BUILD=1 \
  record F3 bash "$EXP/cross_session/run_cross_session_decay.sh"
no_cargo

echo "[freshness] G: CARGO_TARGET_DIR cannot divert any wrapper away from what the driver reads"
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 CARGO_TARGET_DIR="$TMP/elsewhere" \
  record G1 bash "$EXP/run_baseline.sh" "$TMP/g1"
target_pinned
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 CARGO_TARGET_DIR="$TMP/elsewhere" \
  record G2 bash "$EXP/run_protected.sh" "$TMP/g2"
target_pinned
MODE=wrap_only OUT_DIR="$TMP/g3" CARGO_TARGET_DIR="$TMP/elsewhere" \
  record G3 bash "$EXP/cross_session/run_cross_session_decay.sh"
target_pinned

echo "[freshness] H: cross-session rejects an out-of-range RUN_LIVE by validation, not by dying later"
# Assert the MESSAGE, not just the status: without validation the script runs on and the driver
# dies of the same bad value, also with status 2, so an exit-code-only assertion passes against
# the very defect it targets.
H_STATUS=0
RUN_LIVE=2 MODE=wrap_only OUT_DIR="$TMP/h" \
  bash "$EXP/cross_session/run_cross_session_decay.sh" >"$TMP/h.log" 2>&1 || H_STATUS=$?
if [[ "$H_STATUS" -eq 2 ]] && grep -q "FAIL: RUN_LIVE must be 0 or 1" "$TMP/h.log"; then
  ok "H: RUN_LIVE=2 rejected up front with exit 2"
else
  bad "H: must be rejected by validation (exit 2 + message), got exit $H_STATUS: $(tail -1 "$TMP/h.log")"
fi

echo "[freshness] I: cross-session wrap_only builds assay-cli and not the guard"
MODE=wrap_only OUT_DIR="$TMP/i" record I bash "$EXP/cross_session/run_cross_session_decay.sh"
succeeded; built "assay-cli"; not_built "assay-mcp-server"; target_pinned

echo "[freshness] J: cross-session sequence_only builds the guard too"
MODE=sequence_only OUT_DIR="$TMP/j" record J bash "$EXP/cross_session/run_cross_session_decay.sh"
succeeded; built "assay-cli"; built "assay-mcp-server"; target_pinned

echo "[freshness] K: cross-session combined builds both"
MODE=combined OUT_DIR="$TMP/k" record K bash "$EXP/cross_session/run_cross_session_decay.sh"
succeeded; built "assay-cli"; built "assay-mcp-server"

echo "[freshness] L: what the wrappers build is what the driver opens"
# The other half of the invariant. Cases A-K only prove cargo was asked for the right thing; if
# the driver were pointed at target/release or any other tree they would all still pass.
DRIVER_PATHS="$(grep -oE 'repo_root / "target/[^"]+"' "$EXP/drive_fragmented_ipi.py" \
  | sed 's/.*"\(.*\)"/\1/' | sort -u)"
if [[ -z "$DRIVER_PATHS" ]]; then
  bad "L: found no target/ paths in the driver; this assertion has gone blind"
else
  while IFS= read -r p; do
    # --target-dir "$ROOT/target" plus the default dev profile yields exactly target/debug/.
    if [[ "$p" == target/debug/* ]]; then
      ok "L: driver opens $p, under the pinned build dir"
    else
      bad "L: driver opens $p, which no wrapper builds into (they pin --target-dir $ROOT/target, dev profile)"
    fi
  done <<< "$DRIVER_PATHS"
fi

echo "[freshness] M: a stale binary is actually rebuilt (real cargo, real driver)"
MAIN_RS="$ROOT/crates/assay-cli/src/main.rs"
MAIN_MTIME_BEFORE="$(mtime_of "$MAIN_RS")"
"$REAL_CARGO" build -q --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target" -p assay-cli
touch "$MAIN_RS"
BEFORE="$(mtime_of "$ROOT/target/debug/assay")"
sleep 1
M_STATUS=0
RUNS_ATTACK=1 RUNS_LEGIT=1 RUN_LIVE=0 bash "$EXP/run_baseline.sh" "$TMP/m" >"$TMP/m.log" 2>&1 || M_STATUS=$?
AFTER="$(mtime_of "$ROOT/target/debug/assay")"
if [[ "$BEFORE" != "$AFTER" ]]; then
  ok "M: stale binary rebuilt ($BEFORE -> $AFTER)"
else
  bad "M: binary NOT rebuilt; an existence check would have passed here"
fi
# The rebuild is necessary but not sufficient: assert the run it fed actually produced its output,
# so a wrapper that builds and then does nothing cannot pass.
if [[ "$M_STATUS" -eq 0 && -s "$TMP/m/summary.json" ]]; then
  ok "M: run produced summary.json"
else
  bad "M: wrapper exited $M_STATUS and/or produced no summary.json: $(tail -2 "$TMP/m.log" | tr '\n' ' ')"
fi
# Leave the tree as found: touching main.rs would force an unrelated rebuild for the next caller.
touch -t "$(date -r "$MAIN_MTIME_BEFORE" +%Y%m%d%H%M.%S 2>/dev/null || date -d "@$MAIN_MTIME_BEFORE" +%Y%m%d%H%M.%S)" \
  "$MAIN_RS" 2>/dev/null || true

echo
if (( FAILURES > 0 )); then
  echo "[freshness] FAILED: $FAILURES assertion(s)"
  exit 1
fi
echo "[freshness] all assertions passed"
