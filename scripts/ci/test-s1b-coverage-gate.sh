#!/usr/bin/env bash
# Contract: coverage gate allows only CLI-reachable labels; mutation/cleanup selftests.
# Pins workflow run bodies and wrapper shape. Does not claim to prove arbitrary execution.
set -euo pipefail
DRIVER="$(cd "$(dirname "$0")" && pwd)/run-send-syscall-matrix.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

assert_nl_snippet() {
  local blob="$1" want="$2" msg="$3"
  [[ $'\n'"$blob"$'\n' == *$'\n'"$want"$'\n'* ]] || fail "$msg"
}

fn_active() {
  awk -v name="$1" '
    $0 ~ ("^" name "\\(\\) \\{") { p=1 }
    p {
      tmp = $0
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp != "" && tmp !~ /^#/) print tmp
    }
    p && /^}/ { exit }
  ' "$DRIVER"
}

# Count every active signature, including `name () {`, `name ( ) {`, and Allman `name()` / `{`.
# fn_active still extracts the canonical `name() {` body.
count_active_fn_sigs() {
  local blob="$1" name="$2"
  printf '%s\n' "$blob" | grep -Ec "^${name}[[:space:]]*\\([[:space:]]*\\)|^function[[:space:]]+${name}([[:space:]]|\\(|$)" || true
}

# One POSIX Python stdlib process-group bound. Not GNU timeout(1).
# Kills the session started for this command only — not a daemon/setsid guarantee.
command -v python3 >/dev/null 2>&1 \
  || fail "coverage-gate hang bound requires python3"
run_bounded() {
  local secs="$1"
  shift
  python3 - "$secs" "$@" <<'PY'
import os
import signal
import subprocess
import sys

secs = float(sys.argv[1])
cmd = sys.argv[2:]
p = subprocess.Popen(cmd, start_new_session=True)
try:
    sys.exit(p.wait(timeout=secs))
except subprocess.TimeoutExpired:
    try:
        os.killpg(p.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        p.wait(timeout=2)
    except subprocess.TimeoutExpired:
        pass
    # Leader exit is not proof the group is empty (TERM-ignoring grandchild).
    try:
        os.killpg(p.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    p.wait()
    sys.stderr.write("FAIL: bounded run exceeded %ss\n" % sys.argv[1])
    sys.exit(124)
PY
}

gpy="$tmp/s1b_gc_$$.py"
printf '%s\n' \
  'import signal' \
  'import time' \
  'signal.signal(signal.SIGTERM, signal.SIG_IGN)' \
  'signal.signal(signal.SIGINT, signal.SIG_IGN)' \
  'time.sleep(30)' >"$gpy"
trap 'pkill -f "$gpy" >/dev/null 2>&1 || true; rm -rf "$tmp"' EXIT
set +e
t0=$(python3 -c 'import time; print(time.time())')
run_bounded 1 bash -c "python3 \"$gpy\" & wait" >/dev/null 2>"$tmp/bound.err"
bound_ec=$?
t1=$(python3 -c 'import time; print(time.time())')
set -e
elapsed=$(python3 -c "print($t1 - $t0)")
[[ "$bound_ec" -eq 124 ]] || fail "run_bounded must expire a parent+grandchild (got $bound_ec)"
python3 -c "import sys; sys.exit(0 if float(sys.argv[1]) < 8 else 1)" "$elapsed" \
  || fail "run_bounded elapsed ${elapsed}s not within the bound"
if pgrep -f "$gpy" >/dev/null 2>&1; then
  fail "run_bounded leaked a grandchild"
fi
pkill -f "$gpy" >/dev/null 2>&1 || true

run() {
  local json="$1" want="$2" ec=0
  set +e
  bash "$DRIVER" coverage-gate "$json" >"$tmp/out" 2>"$tmp/err"
  ec=$?
  set -e
  if [[ "$want" == "0" ]]; then
    [[ "$ec" -eq 0 ]] || fail "$json: expected exit 0 got $ec stderr=$(cat "$tmp/err")"
  else
    [[ "$ec" -ne 0 ]] || fail "$json: expected nonzero, got 0 stdout=$(cat "$tmp/out")"
  fi
}

printf '%s\n' '{"network_protocol_coverage":"connect_only"}' >"$tmp/connect.json"
printf '%s\n' '{"network_protocol_coverage":"absent"}' >"$tmp/absent.json"
printf '%s\n' '{}' >"$tmp/missing.json"
printf '%s\n' '{"network_protocol_coverage":null}' >"$tmp/null.json"
printf '%s\n' '{"network_protocol_coverage":"datagram_peer_observed"}' >"$tmp/dgram.json"
printf '%s\n' '{"network_protocol_coverage":"connect_and_datagram_peer_observed"}' >"$tmp/both.json"
printf '%s\n' '{"network_protocol_coverage":"quic_peer_observed"}' >"$tmp/future.json"

run "$tmp/connect.json" 0
run "$tmp/absent.json" 0
run "$tmp/missing.json" nz
run "$tmp/null.json" nz
run "$tmp/dgram.json" nz
run "$tmp/both.json" nz
run "$tmp/future.json" nz
echo "ok: coverage-gate contract"

mut_out=$(bash "$DRIVER" mutation-selftest)
[[ "$mut_out" == "ok: mutation uniqueness, restore, into-fails, to_string-compiles" ]] \
  || fail "mutation-selftest stdout must be exactly the uniqueness marker (got: ${mut_out:-<empty>})"

run_mode() {
  local mode="$1" want="$2"
  local ec=0 label
  shift 2
  label="$mode $*"
  set +e
  bash "$DRIVER" "$mode" "$@" >"$tmp/out" 2>"$tmp/err"
  ec=$?
  set -e
  if [[ "$want" == "0" ]]; then
    [[ "$ec" -eq 0 ]] || fail "$label: expected 0 got $ec stderr=$(cat "$tmp/err")"
    grep -q '^ok:' "$tmp/out" || fail "$label: missing ok: stdout=$(cat "$tmp/out")"
  else
    [[ "$ec" -ne 0 ]] || fail "$label: expected nonzero, got 0 stdout=$(cat "$tmp/out")"
    if grep -q 'usage:' "$tmp/err"; then
      fail "$label: failed as unknown mode, not a check: $(cat "$tmp/err")"
    fi
  fi
}

printf '[PID 1] sendto: 127.0.0.1:8080\n' >"$tmp/ep_8080.log"
printf '[PID 1] sendto: 127.0.0.1:80\n' >"$tmp/ep_80.log"
printf '[PID 99] sendto: 127.0.0.1:80\n' >"$tmp/ep_wrong_pid.log"
printf '[PID 1] connect: 127.0.0.1:80\n' >"$tmp/ep_wrong_op.log"
run_mode endpoint-line-selftest nz "$tmp/ep_8080.log" "1" "sendto" "127.0.0.1" "80"
run_mode endpoint-line-selftest nz "$tmp/ep_wrong_pid.log" "1" "sendto" "127.0.0.1" "80"
run_mode endpoint-line-selftest nz "$tmp/ep_wrong_op.log" "1" "sendto" "127.0.0.1" "80"
run_mode endpoint-line-selftest 0 "$tmp/ep_80.log" "1" "sendto" "127.0.0.1" "80"
echo "ok: endpoint-line fixtures"

printf 'NOT_HARNESS_OK\n' >"$tmp/h_not"
printf 'foo HARNESS_OK bar\n' >"$tmp/h_sub"
printf 'HARNESS_OK\n' >"$tmp/h_ok"
run_mode harness-ok-selftest nz "$tmp/h_not"
run_mode harness-ok-selftest nz "$tmp/h_sub"
run_mode harness-ok-selftest 0 "$tmp/h_ok"
echo "ok: harness-ok fixtures"

# Grammar from monitor_next/mod.rs emit_err! of
# "  • Tracepoint ringbuf: emitted={} dropped={}" (not output.rs).
rb_prefix='  • Tracepoint ringbuf: emitted='
printf '%s\n' \
  "${rb_prefix}1 dropped=0" \
  "${rb_prefix}1 dropped=1" >"$tmp/rb_mixed.log"
printf '%s\n' 'send observation only, no ringbuf summary' >"$tmp/rb_none.log"
printf '%s\n' "${rb_prefix}1 dropped=0" >"$tmp/rb_one.log"
printf '%s\n' \
  "${rb_prefix}1 dropped=0" \
  "${rb_prefix}2 dropped=0" >"$tmp/rb_two.log"
printf '%s\n' "${rb_prefix}1 dropped=01" >"$tmp/rb_01.log"
run_mode ringbuf-drop-selftest nz "$tmp/rb_mixed.log"
run_mode ringbuf-drop-selftest nz "$tmp/rb_none.log"
run_mode ringbuf-drop-selftest nz "$tmp/rb_01.log"
run_mode ringbuf-drop-selftest 0 "$tmp/rb_one.log"
run_mode ringbuf-drop-selftest 0 "$tmp/rb_two.log"
echo "ok: ringbuf-drop fixtures"

cli_mod="$(cd "$(dirname "$0")/../.." && pwd)/crates/assay-cli/src/cli/commands/monitor_next/mod.rs"
grep -Fq "${rb_prefix}{} dropped={}" "$cli_mod" \
  || fail "ringbuf formatter prefix missing from monitor_next/mod.rs"

cli_out="$(cd "$(dirname "$0")/../.." && pwd)/crates/assay-cli/src/cli/commands/monitor_next/output.rs"
so_fmt='  • Send observation:   sendto emitted={} dropped={} no_peer={} non_ip={}; sendmsg emitted={} dropped={} no_peer={} non_ip={}'
so_fn=$(awk '
  /fn format_send_observation_summary\(/ { p=1 }
  p { print }
  p && /^}$/ { exit }
' "$cli_out")
[[ -n "$so_fn" ]] || fail "could not extract format_send_observation_summary body"
grep -Fq "$so_fmt" <<<"$so_fn" \
  || fail "send observation formatter missing from format_send_observation_summary body"
so_printf="${so_fmt//'{}'/%s}"
grep -Fq "$so_printf" "$DRIVER" \
  || fail "driver send_observation_ok printf drifted from output.rs producer"
so_line() {
  local f="$so_fmt" v
  for v in "$@"; do
    f="${f/\{\}/$v}"
  done
  printf '%s\n' "$f"
}
pos="$(so_line 1 0 1 1 1 0 1 1)"
dis="$(so_line 0 0 0 0 0 0 0 0)"
pos="${pos%$'\n'}"
dis="${dis%$'\n'}"
printf '%s\n' "$pos" >"$tmp/so_pos_ok.log"
printf '%s extra=9\n' "$pos" >"$tmp/so_pos_suffix.log"
printf '%s; bogus=1\n' "$pos" >"$tmp/so_pos_token.log"
printf '%s\n' "$dis" >"$tmp/so_dis_ok.log"
printf '%s extra=9\n' "$dis" >"$tmp/so_dis_suffix.log"
printf '%s; bogus=1\n' "$dis" >"$tmp/so_dis_token.log"
run_mode send-observation-selftest nz "$tmp/so_pos_suffix.log" 1 0 1 1 1 0 1 1
run_mode send-observation-selftest nz "$tmp/so_pos_token.log" 1 0 1 1 1 0 1 1
run_mode send-observation-selftest nz "$tmp/so_dis_suffix.log" 0 0 0 0 0 0 0 0
run_mode send-observation-selftest nz "$tmp/so_dis_token.log" 0 0 0 0 0 0 0 0
run_mode send-observation-selftest 0 "$tmp/so_pos_ok.log" 1 0 1 1 1 0 1 1
run_mode send-observation-selftest 0 "$tmp/so_dis_ok.log" 0 0 0 0 0 0 0 0
echo "ok: send-observation fixtures"

run_mode monitor-shutdown-selftest 0 0
run_mode monitor-shutdown-selftest nz 1
run_mode monitor-shutdown-selftest nz 130
run_mode monitor-shutdown-selftest nz 139
grep -Fq 'tokio::signal::ctrl_c()' "$cli_mod" \
  || fail "monitor SIGINT path missing tokio::signal::ctrl_c"
driver_active=$(awk '
  {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp == "" || tmp ~ /^#/) next
    print tmp
  }
' "$DRIVER")
n_run_matrix=$(count_active_fn_sigs "$driver_active" run_matrix)
[[ "$n_run_matrix" -eq 1 ]] || fail "driver must define run_matrix() exactly once"
n_assert_effects=$(count_active_fn_sigs "$driver_active" assert_matrix_effects)
[[ "$n_assert_effects" -eq 1 ]] || fail "driver must define assert_matrix_effects() exactly once"
n_wait_endpoint=$(count_active_fn_sigs "$driver_active" wait_endpoint)
[[ "$n_wait_endpoint" -eq 1 ]] || fail "driver must define wait_endpoint() exactly once"
n_emit_diag=$(count_active_fn_sigs "$driver_active" emit_matrix_diagnostics)
[[ "$n_emit_diag" -eq 1 ]] || fail "driver must define emit_matrix_diagnostics() exactly once"
n_fail=$(count_active_fn_sigs "$driver_active" fail)
[[ "$n_fail" -eq 1 ]] || fail "driver must define fail() exactly once"
python3 - "$DRIVER" "$tmp/mut-space-run.sh" "$tmp/mut-space-assert.sh" "$tmp/mut-allman-run.sh" <<'PY'
from pathlib import Path
import sys
src = Path(sys.argv[1]).read_text()
old = 'case "$MODE" in\n'
if old not in src:
    raise SystemExit("driver is missing case \"$MODE\" in")
Path(sys.argv[2]).write_text(
    src.replace(old, 'run_matrix ( ) { echo "ok: $MODE matrix"; return 0; }\n' + old, 1)
)
Path(sys.argv[3]).write_text(
    src.replace(old, 'assert_matrix_effects () { :; }\n' + old, 1)
)
Path(sys.argv[4]).write_text(
    src.replace(
        old,
        'run_matrix()\n{\n  echo "ok: $MODE matrix"\n  return 0\n}\n' + old,
        1,
    )
)
PY
active_of() {
  awk '
    {
      tmp = $0
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) next
      print tmp
    }
  ' "$1"
}
n=$(count_active_fn_sigs "$(active_of "$tmp/mut-space-run.sh")" run_matrix)
[[ "$n" -ne 1 ]] || fail "space-form later run_matrix() was not counted"
n=$(count_active_fn_sigs "$(active_of "$tmp/mut-space-assert.sh")" assert_matrix_effects)
[[ "$n" -ne 1 ]] || fail "space-form later assert_matrix_effects() was not counted"
n=$(count_active_fn_sigs "$(active_of "$tmp/mut-allman-run.sh")" run_matrix)
[[ "$n" -ne 1 ]] || fail "Allman later run_matrix() was not counted"
run_matrix_active=$(fn_active run_matrix)
# shellcheck disable=SC2016
want_run_matrix=$(cat <<'EOF'
run_matrix() {
local expect_send="$1" n=0 hpid="" hc=0 mc=0 p2 p3
mkdir -p "$WORKDIR"
FIFO="$WORKDIR/go.fifo"
LOG="$WORKDIR/monitor.log"
HOUT="$WORKDIR/harness.out"
OH="$WORKDIR/observation-health.json"
rm -f "$FIFO" "$LOG" "$HOUT" "$OH"
mkfifo "$FIFO"
echo "kernel=$(uname -r) host=$(uname -n) mode=$MODE"
echo "object=$(sha256sum "$ASSAY_EBPF" | awk '{print $1}')"
"$HARNESS_BIN" "$FIFO" >"$HOUT" 2>&1 &
HARNESS_PID=$!
while (( n < 40 )); do
hpid="$(awk -F= '/^HARNESS_PID=/{print $2; exit}' "$HOUT" 2>/dev/null || true)"
[[ -n "$hpid" ]] && break
sleep 0.1
n=$((n + 1))
done
[[ -n "$hpid" && "$hpid" == "$HARNESS_PID" ]] || fail "harness PID missing or mismatch"
LEAF="$(isolate_pid "$hpid")"
echo "isolated pid $hpid into $LEAF"
"$ASSAY_BIN" monitor --pid "$hpid" --ebpf "$ASSAY_EBPF" --print \
--observation-health "$OH" >"$LOG" 2>&1 &
MONITOR_PID=$!
wait_log "Assay Monitor running"
wait_log "DEBUG: Attached Tracepoint sys_enter_connect"
if [[ "$expect_send" == "yes" ]]; then
wait_log "DEBUG: Attached Tracepoint sys_enter_sendto"
wait_log "DEBUG: Attached Tracepoint sys_enter_sendmsg"
elif send_debug; then
fail "attach-disabled run still attached send tracepoints"
fi
kill -0 "$HARNESS_PID" 2>/dev/null || fail "harness died before GO"
write_go
wait "$HARNESS_PID" || hc=$?
HARNESS_PID=""
if [[ "$expect_send" == "yes" ]]; then
p2="$(awk -F= '/^CELL2_UDP_PORT=/{print $2; exit}' "$HOUT")"
p3="$(awk -F= '/^CELL3_UDP_PORT=/{print $2; exit}' "$HOUT")"
[[ -n "$p2" && -n "$p3" ]] || fail "missing send ports in harness stdout"
wait_endpoint "$hpid" sendto "127.0.0.1" "$p2"
wait_endpoint "$hpid" sendmsg "127.0.0.1" "$p3"
fi
kill -INT "$MONITOR_PID" 2>/dev/null || true
wait "$MONITOR_PID" || mc=$?
MONITOR_PID=""
echo "monitor exit=$mc"
assert_matrix_effects "$expect_send" "$hpid" "$hc" "$mc"
}
EOF
)
[[ "$run_matrix_active" == "$want_run_matrix" ]] \
  || fail "run_matrix active body must be the closed startup-then-assert shape"
wait_ep_block=$(printf '%s\n' \
  'wait_endpoint "$hpid" sendto "127.0.0.1" "$p2"' \
  'wait_endpoint "$hpid" sendmsg "127.0.0.1" "$p3"')
[[ "$run_matrix_active" == *"$wait_ep_block"* ]] \
  || fail "run_matrix must wait for sendto/sendmsg via wait_endpoint before SIGINT"
python3 - "$DRIVER" "$tmp/mut-wait-driver.sh" <<'PY'
from pathlib import Path
import sys
src, dst = Path(sys.argv[1]), Path(sys.argv[2])
text = src.read_text()
old = '''    wait_endpoint "$hpid" sendto "127.0.0.1" "$p2"
    wait_endpoint "$hpid" sendmsg "127.0.0.1" "$p3"
'''
if old not in text:
    raise SystemExit("driver is missing the pre-SIGINT wait_endpoint calls")
dst.write_text(text.replace(old, "", 1))
PY
mut_run_matrix=$(awk '
  $0 ~ "^run_matrix\\(\\) \\{" { p=1 }
  p {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp != "" && tmp !~ /^#/) print tmp
  }
  p && /^}/ { exit }
' "$tmp/mut-wait-driver.sh")
[[ "$mut_run_matrix" != "$want_run_matrix" ]] \
  || fail "removing pre-SIGINT wait_endpoint was not caught by the run_matrix pin"
assert_effects_active=$(fn_active assert_matrix_effects)
# shellcheck disable=SC2016
want_assert_effects=$(cat <<'EOF'
assert_matrix_effects() {
local expect_send="$1" hpid="$2" hc="$3" mc="$4" tcp p2 p3 b summary
[[ "$hc" -eq 0 ]] || fail "harness exit $hc (receiver effect missing)"
harness_ok "$HOUT" || fail "harness did not print HARNESS_OK"
monitor_shutdown_ok "$mc" || fail "monitor exit $mc is not controlled SIGINT shutdown (expected 0)"
tcp="$(awk -F= '/^CELL1_TCP_PORT=/{print $2; exit}' "$HOUT")"
p2="$(awk -F= '/^CELL2_UDP_PORT=/{print $2; exit}' "$HOUT")"
p3="$(awk -F= '/^CELL3_UDP_PORT=/{print $2; exit}' "$HOUT")"
[[ -n "$tcp" && -n "$p2" && -n "$p3" ]] || fail "missing bound ports in harness stdout"
grep -q "CELL_OK 1 accept" "$HOUT" || fail "cell 1 accept missing"
for b in a2 a3 a4 a5 a6 a7; do
grep -qi "CELL_OK recv=0x${b}" "$HOUT" || fail "receiver byte 0x${b} missing"
done
endpoint_line_ok "$LOG" "$hpid" "connect" "127.0.0.1" "$tcp" \
|| fail "cell 1 connect line missing for 127.0.0.1:${tcp}"
summary="$(grep 'Send observation:' "$LOG" || true)"
[[ -n "$summary" ]] || fail "missing Send observation summary"
if [[ "$expect_send" == "yes" ]]; then
endpoint_line_ok "$LOG" "$hpid" "sendto" "127.0.0.1" "$p2" \
|| fail "cell 2 sendto endpoint missing"
endpoint_line_ok "$LOG" "$hpid" "sendmsg" "127.0.0.1" "$p3" \
|| fail "cell 3 sendmsg endpoint missing"
[[ "$(grep -c "\\[PID ${hpid}\\] sendto:" "$LOG")" -eq 1 &&
"$(grep -c "\\[PID ${hpid}\\] sendmsg:" "$LOG")" -eq 1 ]] ||
fail "expected exactly one sendto and one sendmsg endpoint line"
send_observation_ok "$summary" 1 0 1 1 1 0 1 1 \
|| fail "exact send counts missing: $summary"
else
if send_debug; then
fail "send DEBUG attach lines present in attach-disabled run"
fi
if grep -Eq "\\[PID ${hpid}\\] send(to|msg):" "$LOG"; then
fail "send endpoint lines present in attach-disabled run"
fi
send_observation_ok "$summary" 0 0 0 0 0 0 0 0 \
|| fail "attach-disabled send stats not all zero: $summary"
fi
ringbuf_drops_ok "$LOG" || fail "tracepoint drop field is not 0"
coverage_gate "$OH"
echo "ok: $MODE matrix"
}
EOF
)
[[ "$assert_effects_active" == "$want_assert_effects" ]] \
  || fail "assert_matrix_effects active body must be the closed effect-half shape"
echo "ok: monitor SIGINT shutdown is exit 0, not 130/crash"

case_active=$(awk '
  /^case "/ { p=1 }
  p {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp != "" && tmp !~ /^#/) print tmp
  }
  p && /^esac$/ { exit }
' "$DRIVER")
# shellcheck disable=SC2016
assert_nl_snippet "$case_active" "$(printf '%s\n' \
  'positive)' \
  '[[ -x "${HARNESS_BIN:-}" && -x "$ASSAY_BIN" && -f "$ASSAY_EBPF" ]] || fail "missing bin/object"' \
  'run_matrix yes ;;')" \
  "positive case arm does not exact-call run_matrix yes"
# shellcheck disable=SC2016
assert_nl_snippet "$case_active" "$(cat <<'EOF'
attach-disabled)
[[ -x "${HARNESS_BIN:-}" && -x "$ASSAY_BIN" && -f "$ASSAY_EBPF" ]] || fail "missing bin/object"
python3 -c 'import pathlib,sys; sys.exit(0 if b"s1b-cell7-disabled" in pathlib.Path(sys.argv[1]).read_bytes() else 1)' \
"$ASSAY_BIN" || fail "ASSAY_BIN is not the mutated rebuild (missing s1b-cell7-disabled)"
run_matrix no ;;
EOF
)" \
  "attach-disabled case arm does not exact-call run_matrix no"
# shellcheck disable=SC2016
assert_nl_snippet "$case_active" "$(printf '%s\n' \
  'diagnostics-selftest)' \
  'LOG="${2:?}"' \
  'HOUT="${3:?}"' \
  'fail "diagnostics-selftest" ;;')" \
  "diagnostics-selftest case arm must call fail after binding LOG and HOUT"
set +e
ASSAY_BIN="$tmp/no-assay" HARNESS_BIN="$tmp/no-harness" ASSAY_EBPF="$tmp/no-ebpf.o" \
  WORKDIR="$tmp/s1b-dispatch-wd" \
  run_bounded 8 bash "$DRIVER" positive >"$tmp/disp.out" 2>"$tmp/disp.err"
disp_ec=$?
set -e
[[ "$disp_ec" -ne 0 ]] || fail "positive with missing bins must not succeed"
if grep -q 'usage:' "$tmp/disp.err"; then
  fail "positive missing-bin failed as unknown mode: $(cat "$tmp/disp.err")"
fi
grep -Fxq 'FAIL: missing bin/object' "$tmp/disp.err" \
  || fail "positive missing-bin must emit exact FAIL: missing bin/object: $(cat "$tmp/disp.err")"
if grep -q '^ok:' "$tmp/disp.out"; then
  fail "positive missing-bin printed ok: $(cat "$tmp/disp.out")"
fi
python3 - "$DRIVER" "$tmp/mut-pos-glob.sh" <<'PY'
from pathlib import Path
import sys
src, dst = Path(sys.argv[1]), Path(sys.argv[2])
text = src.read_text()
old = "  positive)\n"
new = """  pos*)
    echo "ok: $MODE matrix"
    exit 0 ;;
  positive)
"""
if old not in text:
    raise SystemExit("driver is missing the positive) arm")
dst.write_text(text.replace(old, new, 1))
PY
set +e
ASSAY_BIN="$tmp/no-assay" HARNESS_BIN="$tmp/no-harness" ASSAY_EBPF="$tmp/no-ebpf.o" \
  WORKDIR="$tmp/s1b-dispatch-wd" \
  run_bounded 8 bash "$tmp/mut-pos-glob.sh" positive >"$tmp/posglob.out" 2>"$tmp/posglob.err"
posglob_ec=$?
set -e
[[ "$posglob_ec" -eq 0 ]] \
  || fail "pos*) mutant must exit 0 (got $posglob_ec) out=$(cat "$tmp/posglob.out") err=$(cat "$tmp/posglob.err")"
grep -qx 'ok: positive matrix' "$tmp/posglob.out" \
  || fail "pos*) mutant must print ok: positive matrix: out=$(cat "$tmp/posglob.out") err=$(cat "$tmp/posglob.err")"
if grep -q 'FAIL:' "$tmp/posglob.err"; then
  fail "pos*) mutant stderr must not contain FAIL: $(cat "$tmp/posglob.err")"
fi
[[ ! -s "$tmp/posglob.err" ]] \
  || fail "pos*) mutant stderr must be empty: $(cat "$tmp/posglob.err")"

wait_iters=$(sed -n 's/^WAIT_LOG_ITERS=//p' "$DRIVER" | head -1)
wait_sleep=$(sed -n 's/^WAIT_LOG_SLEEP_S=//p' "$DRIVER" | head -1)
wait_max=$(sed -n 's/^WAIT_LOG_MAX_BEFORE_GO=//p' "$DRIVER" | head -1)
go_margin=$(sed -n 's/^GO_TIMEOUT_MARGIN_MS=//p' "$DRIVER" | head -1)
go_ms=$(sed -n 's/^#define GO_FIFO_TIMEOUT_MS //p' "$(cd "$(dirname "$0")" && pwd)/s1b-send-syscall-matrix.c" | head -1)
[[ -n "$wait_iters" && -n "$wait_sleep" && -n "$wait_max" && -n "$go_margin" && -n "$go_ms" ]] \
  || fail "missing GO/wait_log bound constants"
need=$(python3 -c "print(int($wait_max * $wait_iters * float('$wait_sleep') * 1000 + $go_margin))")
[[ "$go_ms" -ge "$need" ]] || fail "GO_FIFO_TIMEOUT_MS=$go_ms < $wait_max*$wait_iters*${wait_sleep}s*1000+$go_margin=$need"
wait_log_active=$(awk '
  /^wait_log\(\)/ { p=1 }
  p {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp != "" && tmp !~ /^#/) print tmp
  }
  p && /^}/ { exit }
' "$DRIVER")
# shellcheck disable=SC2016
want_wait_log=$(printf '%s\n' \
  'wait_log() {' \
  'local pat="$1" n' \
  'for ((n = 0; n < WAIT_LOG_ITERS; n++)); do' \
  'grep -q -- "$pat" "$LOG" && return 0' \
  'if ! kill -0 "$MONITOR_PID" 2>/dev/null; then' \
  'cat "${LOG:-}" >&2 || true' \
  'fail "monitor exited before matching: $pat"' \
  'fi' \
  'sleep "$WAIT_LOG_SLEEP_S"' \
  'done' \
  'cat "${LOG:-}" >&2 || true' \
  'fail "timeout waiting for: $pat"' \
  '}')
[[ "$wait_log_active" == "$want_wait_log" ]] \
  || fail "wait_log active body must be the closed fail-closed shape"
wait_endpoint_active=$(fn_active wait_endpoint)
# shellcheck disable=SC2016
want_wait_endpoint=$(printf '%s\n' \
  'wait_endpoint() {' \
  'local pid="$1" op="$2" ip="$3" port="$4" n' \
  'for ((n = 0; n < WAIT_LOG_ITERS; n++)); do' \
  'endpoint_line_ok "$LOG" "$pid" "$op" "$ip" "$port" && return 0' \
  'if ! kill -0 "$MONITOR_PID" 2>/dev/null; then' \
  'fail "monitor exited before matching: $op $ip:$port"' \
  'fi' \
  'sleep "$WAIT_LOG_SLEEP_S"' \
  'done' \
  'fail "timeout waiting for: $op $ip:$port"' \
  '}')
[[ "$wait_endpoint_active" == "$want_wait_endpoint" ]] \
  || fail "wait_endpoint active body must boundedly wait via endpoint_line_ok"
fail_active=$(fn_active fail)
# shellcheck disable=SC2016
want_fail=$(printf '%s\n' \
  'fail() {' \
  'echo "FAIL: $*" >&2' \
  'emit_matrix_diagnostics' \
  'exit 1' \
  '}')
[[ "$fail_active" == "$want_fail" ]] \
  || fail "fail() must emit matrix diagnostics before exit"
emit_active=$(fn_active emit_matrix_diagnostics)
# shellcheck disable=SC2016
want_emit=$(printf '%s\n' \
  'emit_matrix_diagnostics() {' \
  'local f path' \
  'for f in LOG HOUT; do' \
  'path="${!f:-}"' \
  '[[ -n "$path" && -f "$path" ]] || continue' \
  'echo "----- begin $f -----" >&2' \
  'sed -n "1,${DIAG_MAX_LINES}p" "$path" >&2 || true' \
  'echo "----- end $f -----" >&2' \
  'done' \
  '}')
[[ "$emit_active" == "$want_emit" ]] \
  || fail "emit_matrix_diagnostics must dump only bounded LOG and HOUT"
diag_max=$(sed -n 's/^DIAG_MAX_LINES=//p' "$DRIVER" | head -1)
[[ "$diag_max" == "200" ]] || fail "DIAG_MAX_LINES must be 200"
printf 'MON_CANARY_S1B\n' >"$tmp/diag-mon.log"
printf 'HAR_CANARY_S1B\n' >"$tmp/diag-har.out"
set +e
bash "$DRIVER" diagnostics-selftest "$tmp/diag-mon.log" "$tmp/diag-har.out" \
  >"$tmp/diag.out" 2>"$tmp/diag.err"
diag_ec=$?
set -e
[[ "$diag_ec" -ne 0 ]] || fail "diagnostics-selftest must exit nonzero"
if grep -q 'usage:' "$tmp/diag.err"; then
  fail "diagnostics-selftest failed as unknown mode: $(cat "$tmp/diag.err")"
fi
grep -Fq -- '----- begin LOG -----' "$tmp/diag.err" \
  || fail "assertion failure must dump monitor log: $(cat "$tmp/diag.err")"
grep -Fq 'MON_CANARY_S1B' "$tmp/diag.err" \
  || fail "monitor log dump missing canary"
grep -Fq -- '----- begin HOUT -----' "$tmp/diag.err" \
  || fail "assertion failure must dump harness output"
grep -Fq 'HAR_CANARY_S1B' "$tmp/diag.err" \
  || fail "harness dump missing canary"
if grep -Fq 'GITHUB_WORKSPACE' "$tmp/diag.err"; then
  fail "diagnostics dumped unrelated workspace content"
fi
python3 - "$DRIVER" "$tmp/mut-fail-driver.sh" <<'PY'
from pathlib import Path
import sys
src, dst = Path(sys.argv[1]), Path(sys.argv[2])
text = src.read_text()
old = "  emit_matrix_diagnostics\n"
if old not in text:
    raise SystemExit("fail() is missing emit_matrix_diagnostics")
dst.write_text(text.replace(old, "", 1))
PY
set +e
bash "$tmp/mut-fail-driver.sh" diagnostics-selftest \
  "$tmp/diag-mon.log" "$tmp/diag-har.out" >"$tmp/mut-diag.out" 2>"$tmp/mut-diag.err"
mut_diag_ec=$?
set -e
[[ "$mut_diag_ec" -ne 0 ]] || fail "mutated fail() must still exit nonzero"
if grep -Fq 'MON_CANARY_S1B' "$tmp/mut-diag.err"; then
  fail "deleting emit_matrix_diagnostics still dumped the monitor log"
fi
if grep -Fq 'HAR_CANARY_S1B' "$tmp/mut-diag.err"; then
  fail "deleting emit_matrix_diagnostics still dumped harness output"
fi
grep -Fq 'open_go_fifo(argv[1], GO_FIFO_TIMEOUT_MS)' \
  "$(cd "$(dirname "$0")" && pwd)/s1b-send-syscall-matrix.c" \
  || fail "harness GO wait does not use GO_FIFO_TIMEOUT_MS"
n_wait=$(awk '
  /^run_matrix\(\)/ { in_fn=1 }
  in_fn && /^}/ { exit }
  in_fn && /write_go/ { seen=1 }
  in_fn && !seen && /wait_log "/ { n++ }
  END { print n+0 }
' "$DRIVER")
[[ "$n_wait" -le "$wait_max" ]] \
  || fail "run_matrix has $n_wait wait_log calls before GO, bound is $wait_max"
echo "ok: harness GO wait covers attach wait_log budget"

set +e
run_bounded 12 bash "$DRIVER" cleanup-selftest >"$tmp/cleanup.out" 2>"$tmp/cleanup.err"
cec=$?
set -e
[[ "$cec" -eq 0 ]] || fail "cleanup-selftest ec=$cec err=$(cat "$tmp/cleanup.err")"
grep -q 'ok: cleanup-selftest' "$tmp/cleanup.out" || fail "cleanup-selftest missing ok"
echo "ok: mutation-selftest and cleanup-selftest"

wf="$(cd "$(dirname "$0")/../.." && pwd)/.github/workflows/monitor-attach-smoke.yml"
named_step() {
  awk -v step="$2" '
    $0 ~ ("^      - name: " step "$") { in_step=1; print; next }
    in_step && /^      - name: / { exit }
    in_step { print }
  ' "$1"
}
active_lines() {
  awk '
    {
      tmp = $0
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) next
      print tmp
    }
  ' <<<"$1"
}
active_run_lines() {
  awk '
    function emit_active(line, tmp) {
      tmp = line
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) return
      print tmp
    }
    /^        run: \|/ { in_run=1; next }
    /^        run: >/ { in_run=1; next }
    /^        run: / {
      sub(/^        run: /, "")
      print
      next
    }
    in_run && /^        [^[:space:]]/ { exit }
    in_run { emit_active($0) }
  ' <<<"$1"
}
normalized_run_body() {
  awk '
    /^        run: \|/ { in_run=1; next }
    /^        run: >/ { in_run=1; next }
    /^        run: / {
      sub(/^        run: /, "")
      print
      exit
    }
    in_run && /^        [^[:space:]]/ { exit }
    in_run {
      line = $0
      sub(/^          /, "", line)
      print line
    }
  ' <<<"$1"
}
assert_single_cmd_step() {
  local name="$1" want="$2" step body n_if
  step=$(named_step "$wf" "$name")
  [[ -n "$step" ]] || fail "missing step: $name"
  if grep -Eq '^        continue-on-error[[:space:]]*:' <<<"$step"; then
    fail "$name must not continue-on-error"
  fi
  if grep -Eq '^        shell:' <<<"$step"; then
    fail "$name must not override shell"
  fi
  n_if=$(grep -c '^        if:' <<<"$step" || true)
  [[ "$n_if" -eq 1 ]] || fail "$name must have exactly one if: (got $n_if)"
  grep -qx "        if: \${{ github.event.inputs.proof_mode == 'send-matrix' }}" <<<"$step" \
    || fail "$name must be send-matrix-conditioned"
  body=$(active_lines "$(normalized_run_body "$step")")
  [[ "$body" == "$want" ]] || fail "$name run body must be exactly: $want (got: ${body:-<empty>})"
}
compile_step=$(named_step "$wf" "Compile syscall harness")
[[ -n "$compile_step" ]] || fail "missing Compile syscall harness step"
assert_single_cmd_step "Compile syscall harness" \
  'bash scripts/ci/s1b-compile-and-contract.sh'
assert_single_cmd_step "Positive syscall/effect matrix" \
  'bash scripts/ci/s1b-positive-matrix.sh'
assert_single_cmd_step "Attach-disabled negative matrix" \
  'bash scripts/ci/s1b-attach-disabled.sh'
# Keep shebang; skip blanks and non-shebang comments. Tiny-script shape, not reachability.
script_active_lines() {
  awk '
    {
      tmp = $0
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "") next
      if (tmp ~ /^#/ && tmp !~ /^#!/) next
      print tmp
    }
  '
}
POS="$(cd "$(dirname "$0")" && pwd)/s1b-positive-matrix.sh"
pos_active=$(script_active_lines <"$POS")
# Closed tiny-script shape (not a proof of arbitrary shell reachability).
# shellcheck disable=SC2016
want_pos=$(printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'ROOT="$(cd "$(dirname "$0")/../.." && pwd)"' \
  'cd "$ROOT"' \
  'test -f target/assay-ebpf.o' \
  'exec sudo -E env HARNESS_BIN="${RUNNER_TEMP:?}/s1b-harness" WORKDIR="${RUNNER_TEMP:?}/s1b-positive" bash scripts/ci/run-send-syscall-matrix.sh positive')
[[ "$pos_active" == "$want_pos" ]] \
  || fail "positive wrapper active lines must match the closed six-line shape ending in exec"
COMPILE="$(cd "$(dirname "$0")" && pwd)/s1b-compile-and-contract.sh"
compile_active=$(script_active_lines <"$COMPILE")
# shellcheck disable=SC2016
want_compile=$(printf '%s\n' \
  '#!/usr/bin/env bash' \
  'set -euo pipefail' \
  'ROOT="$(cd "$(dirname "$0")/../.." && pwd)"' \
  'cd "$ROOT"' \
  'out="${RUNNER_TEMP:-/tmp}/s1b-harness"' \
  'cc -Wall -Werror -o "$out" scripts/ci/s1b-send-syscall-matrix.c' \
  '"$out" --timeout-selftest' \
  'fifo="${RUNNER_TEMP:-/tmp}/s1b-fifo-selftest"' \
  'rm -f "$fifo"' \
  'mkfifo "$fifo"' \
  '"$out" --fifo-selftest "$fifo"' \
  'rm -f "$fifo"' \
  'bash scripts/ci/test-s1b-coverage-gate.sh')
[[ "$compile_active" == "$want_compile" ]] \
  || fail "compile wrapper active lines must match the closed tiny-script shape"
smoke_job=$(awk '
  $0 == "  smoke:" { p=1 }
  p && /^  [A-Za-z0-9_-]+:/ && $0 != "  smoke:" { exit }
  p { print }
' "$wf")
smoke_meta=$(awk '/^    steps:/{exit} {print}' <<<"$smoke_job")
smoke_meta_active=$(active_lines "$smoke_meta")
if grep -Eq '^if:' <<<"$smoke_meta_active"; then
  fail "smoke job must not have if"
fi
if grep -Eq '^continue-on-error:' <<<"$smoke_meta_active"; then
  fail "smoke job must not continue-on-error"
fi
pm_opts=$(awk '
  /^      proof_mode:/ { p=1 }
  p && /^      [a-z]/ && $0 !~ /^      proof_mode:/ { exit }
  p && /^        options:/ { o=1; next }
  o && /^        [a-z]/ { exit }
  o { print }
' "$wf")
n_sm=$(active_lines "$pm_opts" | grep -Fcx -- '- send-matrix' || true)
[[ "$n_sm" -eq 1 ]] || fail "proof_mode choices must contain send-matrix exactly once"
pc="$(cd "$(dirname "$0")/../.." && pwd)/.pre-commit-config.yaml"
hook_active=$(awk '
  $0 == "      - id: s1b-coverage-gate-contract" { p=1 }
  p && $0 ~ /^      - id: / && $0 != "      - id: s1b-coverage-gate-contract" { exit }
  p {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp == "" || tmp ~ /^#/) next
    print tmp
  }
' "$pc")
[[ -n "$hook_active" ]] || fail "missing s1b-coverage-gate-contract hook"
grep -qx 'entry: bash scripts/ci/test-s1b-coverage-gate.sh' <<<"$hook_active" \
  || fail "active entry drifted"
grep -qx 'language: system' <<<"$hook_active" || fail "language must be system"
grep -qx 'pass_filenames: false' <<<"$hook_active" \
  || fail "pass_filenames must be false"
if grep -Eq '^stages:' <<<"$hook_active"; then
  fail "s1b hook must not set stages"
fi
if grep -Eq '^exclude:' <<<"$hook_active"; then
  fail "s1b hook must not set exclude"
fi
files_pat=$(awk '/^files: / { sub(/^files: /, ""); print; exit }' <<<"$hook_active")
[[ -n "$files_pat" ]] || fail "files regex omits the send-observation producer"
mkdir -p "$tmp/norb"
ln -sfn "$(command -v python3)" "$tmp/norb/python3"
if PATH="$tmp/norb" command -v ruby >/dev/null 2>&1; then
  fail "ruby-free PATH still locates ruby"
fi
PATH="$tmp/norb" python3 -c '
import re, sys
cre = re.compile(sys.argv[1])
for p in sys.argv[2:]:
    if cre.search(p) is None:
        raise SystemExit("files regex omits " + p)
if cre.search("crates/assay-cli/src/lib.rs") is not None:
    raise SystemExit("files regex is not producer-scoped")
' "$files_pat" \
  "scripts/ci/test-s1b-coverage-gate.sh" \
  "scripts/ci/run-send-syscall-matrix.sh" \
  "scripts/ci/s1b-compile-and-contract.sh" \
  "scripts/ci/s1b-positive-matrix.sh" \
  "scripts/ci/s1b-attach-disabled.sh" \
  "scripts/ci/s1b-send-syscall-matrix.c" \
  ".github/workflows/monitor-attach-smoke.yml" \
  "crates/assay-cli/src/cli/commands/monitor_next/output.rs" \
  "crates/assay-cli/src/cli/commands/monitor_next/mod.rs" \
  "crates/assay-monitor/src/loader.rs" \
  ".github/workflows/kernel-matrix.yml" \
  ".pre-commit-config.yaml" \
  || fail "s1b pre-commit hook YAML contract failed without ruby on PATH"
echo "ok: s1b hook YAML contract without ruby"
km="$(cd "$(dirname "$0")/../.." && pwd)/.github/workflows/kernel-matrix.yml"
pr_paths=$(awk '
  /^  pull_request:/ { p=1; next }
  p && /^  [a-z]/ { exit }
  p { print }
' "$km")
active_pr_paths=$(awk '
  {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp == "" || tmp ~ /^#/) next
    print tmp
  }
' <<<"$pr_paths")
grep -qx -- '- ".github/workflows/monitor-attach-smoke.yml"' <<<"$active_pr_paths" \
  || fail "kernel-matrix pull_request.paths must include monitor-attach-smoke.yml"
if grep -q 'bash scripts/ci/run-send-syscall-matrix.sh cleanup-selftest' "$wf"; then
  fail "workflow still invokes cleanup-selftest directly"
fi
if grep -q 'bash scripts/ci/run-send-syscall-matrix.sh mutation-selftest' "$wf"; then
  fail "workflow still invokes mutation-selftest directly"
fi
a10=$(mktemp -d)
printf '#!/bin/sh\nexit 0\n' >"$a10/assay"
printf '#!/bin/sh\nexit 0\n' >"$a10/harness"
: >"$a10/ebpf.o"
chmod +x "$a10/assay" "$a10/harness"
if grep -Fq 's1b-cell7-disabled' "$a10/assay"; then
  fail "A10 fixture ASSAY_BIN must lack the mutation marker"
fi
set +e
ASSAY_BIN="$a10/assay" HARNESS_BIN="$a10/harness" ASSAY_EBPF="$a10/ebpf.o" \
  run_bounded 8 bash "$DRIVER" attach-disabled >"$tmp/a10.out" 2>"$tmp/a10.err"
a10ec=$?
set -e
rm -rf "$a10"
[[ "$a10ec" -ne 0 ]] || fail "attach-disabled accepted unmarked ASSAY_BIN"
grep -Fq 'not the mutated rebuild' "$tmp/a10.err" \
  || fail "attach-disabled must reject unmarked ASSAY_BIN before run_matrix: $(cat "$tmp/a10.err")"
echo "ok: attach-disabled rejects unmarked ASSAY_BIN"
# First smoke step is Pre-clean; its YAML run: body (not the whole workflow)
# must contain pkill/chattr/sudo find-delete/fail-closed empty, and that step
# must sit immediately before actions/checkout. Global grep is prefix-vacuous.
step_names=$(awk '
  /^    steps:/ { s=1; next }
  s && /^  [a-z]/ { exit }
  s && /^      - name: / {
    sub(/^      - name: /, "")
    print
  }
' "$wf")
first=$(printf '%s\n' "$step_names" | sed -n '1p')
second=$(printf '%s\n' "$step_names" | sed -n '2p')
[[ "$first" == "Pre-clean self-hosted workspace" ]] \
  || fail "first smoke step must be Pre-clean self-hosted workspace, got ${first:-<empty>}"
[[ "$second" == "Checkout" ]] \
  || fail "pre-clean must immediately precede Checkout, got ${second:-<empty>}"
compile_idx=$(printf '%s\n' "$step_names" | grep -n '^Compile syscall harness$' | head -1 | cut -d: -f1)
positive_idx=$(printf '%s\n' "$step_names" | grep -n '^Positive syscall/effect matrix$' | head -1 | cut -d: -f1)
disabled_idx=$(printf '%s\n' "$step_names" | grep -n '^Attach-disabled negative matrix$' | head -1 | cut -d: -f1)
[[ -n "$compile_idx" && -n "$positive_idx" && "$compile_idx" -lt "$positive_idx" ]] \
  || fail "coverage-gate suite step must run before the positive matrix"
[[ -n "$disabled_idx" && "$positive_idx" -lt "$disabled_idx" ]] \
  || fail "positive matrix must run before attach-disabled"
for s in "Compile syscall harness" "Positive syscall/effect matrix" "Attach-disabled negative matrix"; do
  n=$(printf '%s\n' "$step_names" | grep -Fcx "$s" || true)
  [[ "$n" -eq 1 ]] || fail "$s must occur exactly once"
done
preclean_step=$(named_step "$wf" "Pre-clean self-hosted workspace")
preclean_active=$(active_run_lines "$preclean_step")
[[ -n "$preclean_active" ]] || fail "could not extract first Pre-clean run: block"
next_until_checkout=$(awk '
  /^      - name: Pre-clean self-hosted workspace$/ { in_step=1; next }
  in_step && /^      - name: / { after=1 }
  after { print }
  after && /uses: actions\/checkout@/ { exit }
' "$wf")
n_names=$(printf '%s\n' "$next_until_checkout" | grep -c '^      - name: ' || true)
[[ "$n_names" -eq 1 ]] || fail "pre-clean must be immediately before actions/checkout, intervening names=$n_names"
grep -q 'uses: actions/checkout@' <<<"$next_until_checkout" \
  || fail "step after Pre-clean must be actions/checkout"
# shellcheck disable=SC2016
grep -Fq 'sudo pkill -x assay' <<<"$preclean_active" || fail "Pre-clean run: must pkill assay"
# shellcheck disable=SC2016
grep -Fq 'sudo chattr -R -i' <<<"$preclean_active" || fail "Pre-clean run: must chattr -i leftover immutable files"
# shellcheck disable=SC2016
grep -Fq 'find "$GITHUB_WORKSPACE" -mindepth 1 -maxdepth 1 -exec sudo rm -rf {} +' <<<"$preclean_active" \
  || fail "Pre-clean run: must sudo-delete all top-level workspace entries"
# shellcheck disable=SC2016
grep -Fq 'find "$GITHUB_WORKSPACE" -mindepth 1 -maxdepth 1 -print -quit' <<<"$preclean_active" \
  || fail "Pre-clean run: must fail-closed assert the workspace is empty"
grep -Fq 'ERROR: workspace not empty after' <<<"$preclean_active" \
  || fail "Pre-clean run: must fail-closed if the workspace is not empty"
grep -Fq 'ERROR: GITHUB_WORKSPACE empty; refusing pre-clean wipe' <<<"$preclean_active" \
  || fail "Pre-clean must fail closed when GITHUB_WORKSPACE is empty"
guard_n=$(printf '%s\n' "$preclean_active" | grep -n 'GITHUB_WORKSPACE empty' | head -1 | cut -d: -f1)
wipe_n=$(printf '%s\n' "$preclean_active" | grep -n 'exec sudo rm -rf' | head -1 | cut -d: -f1)
[[ -n "$guard_n" && -n "$wipe_n" && "$guard_n" -lt "$wipe_n" ]] \
  || fail "empty GITHUB_WORKSPACE guard must precede sudo deletion"
guard_to_wipe=$(awk '
  /GITHUB_WORKSPACE empty/ { p=1 }
  p { print }
  /exec sudo rm -rf/ { exit }
' <<<"$preclean_active")
if grep -Eq '^(GITHUB_WORKSPACE=|unset GITHUB_WORKSPACE)' <<<"$guard_to_wipe"; then
  fail "GITHUB_WORKSPACE must not be emptied between the empty-workspace guard and sudo deletion"
fi
nonempty_branch=$(awk '
  /mindepth 1 -maxdepth 1 -print -quit/ { p=1 }
  p { print }
  p && /[[:space:]]fi[[:space:]]*$/ { exit }
' <<<"$preclean_active")
grep -Eq '^[[:space:]]*exit 1[[:space:]]*$' <<<"$nonempty_branch" \
  || fail "Pre-clean nonempty branch must fail-closed with exit 1"
if grep -Eq '^        continue-on-error[[:space:]]*:' <<<"$preclean_step"; then
  fail "Pre-clean must not continue-on-error"
fi
if grep -Eq '^        if:' <<<"$preclean_step"; then
  fail "Pre-clean must not have a disabling if"
fi
mkdir -p "$tmp/preclean-bin"
: >"$tmp/preclean-sudo.log"
printf '#!/bin/sh\necho MOCKSUDO "$@" >>%s\n' "$tmp/preclean-sudo.log" >"$tmp/preclean-bin/sudo"
chmod +x "$tmp/preclean-bin/sudo"
{
  echo '#!/usr/bin/env bash'
  echo 'set +e'
  printf '%s\n' "$preclean_active"
} >"$tmp/preclean-replay.sh"
set +e
GITHUB_WORKSPACE='' \
  PATH="$tmp/preclean-bin:/usr/bin:/bin" bash "$tmp/preclean-replay.sh" \
  >"$tmp/preclean-empty.out" 2>"$tmp/preclean-empty.err"
gw_ec=$?
set -e
[[ "$gw_ec" -ne 0 ]] || fail "empty GITHUB_WORKSPACE must fail closed before sudo deletion"
if grep -q 'rm -rf' "$tmp/preclean-sudo.log"; then
  fail "empty GITHUB_WORKSPACE reached sudo rm"
fi
: >"$tmp/preclean-sudo.log"
ws="$tmp/preclean-ws"
mkdir -p "$ws/residue"
printf 'leftover\n' >"$ws/residue/keep"
set +e
GITHUB_WORKSPACE="$ws" \
  PATH="$tmp/preclean-bin:/usr/bin:/bin" bash "$tmp/preclean-replay.sh" \
  >"$tmp/preclean-ne.out" 2>"$tmp/preclean-ne.err"
ne_ec=$?
set -e
[[ "$ne_ec" -ne 0 ]] || fail "pre-clean must fail when mock sudo leaves residue"
grep -Fq 'ERROR: workspace not empty after delegated pre-clean' \
  "$tmp/preclean-ne.out" "$tmp/preclean-ne.err" \
  || fail "pre-clean residue must emit the workspace-not-empty diagnostic"
[[ -f "$ws/residue/keep" ]] || fail "nonempty pre-clean replay lost its planted residue"
echo "ok: pre-clean run: block scoped before checkout"
ATTACH="$(cd "$(dirname "$0")" && pwd)/s1b-attach-disabled.sh"
attach_active=$(active_lines "$(cat "$ATTACH")")
n_restore=$(printf '%s\n' "$attach_active" | grep -Ec '^restore\(\) \{|^function restore' || true)
[[ "$n_restore" -eq 1 ]] || fail "attach-disabled must define restore() exactly once"
grep -qx 'readonly -f restore' <<<"$attach_active" \
  || fail "restore must be readonly so an alternate definition cannot replace it"
last_exit=$(printf '%s\n' "$attach_active" | grep -E '^trap .* EXIT' | tail -1)
[[ "$last_exit" == "trap restore EXIT" ]] || fail "effective EXIT trap must be restore"
if printf '%s\n' "$attach_active" | grep -Eq '^trap - EXIT$|^trap '\'''\'' EXIT$|^trap "" EXIT$'; then
  fail "EXIT trap must not be disarmed"
fi
restore_fn=$(awk '/^restore\(\) \{/,/^}$/' "$ATTACH")
[[ -n "$restore_fn" ]] || fail "could not extract restore() from attach-disabled wrapper"
grep -Fq 'REAL_BIN_PATH' <<<"$restore_fn" \
  || fail "restore must write the captured REAL_BIN_PATH, not a rebound bin"
if grep -q 'cargo build' <<<"$restore_fn"; then
  fail "attach-disabled restore must not cargo build"
fi
prod_active=$(awk '
  /^if \[\[ "\$\{1:-\}" == restore-selftest/ { skip=1 }
  skip && /^fi$/ { skip=0; next }
  skip { next }
  { print }
' "$ATTACH" | script_active_lines)
# shellcheck disable=SC2016
want_attach_prod=$(cat <<'EOF'
#!/usr/bin/env bash
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
EOF
)
[[ "$prod_active" == "$want_attach_prod" ]] \
  || fail "attach-disabled production active lines must match the closed shape ending in attach-disabled"
last_prod=$(printf '%s\n' "$prod_active" | tail -1)
[[ "$last_prod" == 'bash scripts/ci/run-send-syscall-matrix.sh attach-disabled' ]] \
  || fail "attach-disabled production path must end in the load-bearing attach-disabled invocation"
if grep -Eq '^bin=' <<<"$prod_active"; then
  fail "production attach-disabled must not rebind bin before restore"
fi
grep -qx 'readonly REAL_SRC_PATH REAL_BIN_PATH bak binbak' <<<"$prod_active" \
  || fail "production restore sources must be readonly after assignment"
ro_n=$(printf '%s\n' "$prod_active" | grep -n '^readonly REAL_SRC_PATH REAL_BIN_PATH bak binbak$' | head -1 | cut -d: -f1)
trap_n=$(printf '%s\n' "$prod_active" | grep -n '^trap restore EXIT$' | head -1 | cut -d: -f1)
[[ -n "$ro_n" && -n "$trap_n" && "$ro_n" -lt "$trap_n" ]] \
  || fail "captured restore paths must be readonly before trap arming"
after_trap=$(awk '/^trap restore EXIT/{p=1; next} p{print}' <<<"$prod_active")
if grep -Eq '^(REAL_BIN_PATH|REAL_SRC_PATH|bak|binbak)=' <<<"$after_trap"; then
  fail "captured restore paths must not be rebound after trap arming"
fi
restore_out=$(bash "$ATTACH" restore-selftest)
[[ "$restore_out" == "ok: restore-selftest" ]] \
  || fail "restore-selftest stdout must be exactly ok: restore-selftest (got: ${restore_out:-<empty>})"
c_src="$(cd "$(dirname "$0")" && pwd)/s1b-send-syscall-matrix.c"
grep -Fq '#define SELFTEST_TIMEOUT_MIN_MS 1500' "$c_src" \
  || fail "SELFTEST_TIMEOUT_MIN_MS must be 1500"
grep -Fq 'MUST(elapsed_ms >= SELFTEST_TIMEOUT_MIN_MS, "timeout selftest elapsed");' "$c_src" \
  || fail "timeout selftest does not assert elapsed_ms >= SELFTEST_TIMEOUT_MIN_MS"
grep -Fq 'MUST(S_ISFIFO(st.st_mode), "fifo selftest not a fifo");' "$c_src" \
  || fail "fifo selftest does not reject a non-FIFO path"
grep -Fq 'MUST(ferr == ETIMEDOUT, "fifo selftest errno");' "$c_src" \
  || fail "fifo selftest does not require ETIMEDOUT"
grep -Fq 'MUST(elapsed_ms >= SELFTEST_TIMEOUT_MIN_MS, "fifo selftest elapsed");' "$c_src" \
  || fail "fifo selftest does not assert elapsed_ms >= SELFTEST_TIMEOUT_MIN_MS"
grep -Fq 'fd = open_go_fifo(argv[2], 2000);' "$c_src" \
  || fail "fifo selftest does not call open_go_fifo with a 2000ms bound"
cc_args=(-Wall -Werror)
if [[ "$(uname -s)" == Darwin ]]; then
  cc_args+=(-Wno-deprecated-declarations)
fi
cc "${cc_args[@]}" -o "$tmp/s1b-harness" "$c_src" 2>"$tmp/cc.err" \
  || fail "fifo-selftest harness failed to compile: $(cat "$tmp/cc.err")"
set +e
"$tmp/s1b-harness" --fifo-selftest "$tmp/missing-fifo" \
  >"$tmp/fifo-miss.out" 2>"$tmp/fifo-miss.err"
miss_ec=$?
set -e
[[ "$miss_ec" -ne 0 ]] || fail "fifo-selftest on a missing path must not succeed"
if grep -q FIFO_TIMEOUT_OK "$tmp/fifo-miss.out" "$tmp/fifo-miss.err"; then
  fail "fifo-selftest on a missing path claimed FIFO_TIMEOUT_OK"
fi
printf 'x\n' >"$tmp/not-a-fifo"
set +e
"$tmp/s1b-harness" --fifo-selftest "$tmp/not-a-fifo" \
  >"$tmp/fifo-reg.out" 2>"$tmp/fifo-reg.err"
reg_ec=$?
set -e
[[ "$reg_ec" -ne 0 ]] || fail "fifo-selftest on a regular file must not succeed"
if grep -q FIFO_TIMEOUT_OK "$tmp/fifo-reg.out" "$tmp/fifo-reg.err"; then
  fail "fifo-selftest on a regular file claimed FIFO_TIMEOUT_OK"
fi
echo "ok: restore copies exact pre-mutation binary; no cargo build in restore"
