#!/usr/bin/env bash
# S1b driver: cgroup/PID isolate, start monitor, GO, assert effects + telemetry.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODE="${1:-}"
ASSAY_BIN="${ASSAY_BIN:-$ROOT/target/release/assay}"
ASSAY_EBPF="${ASSAY_EBPF:-$ROOT/target/assay-ebpf.o}"
HARNESS_BIN="${HARNESS_BIN:-}"
WORKDIR="${WORKDIR:-${RUNNER_TEMP:-/tmp}/s1b-send-matrix}"

fail() { echo "FAIL: $*" >&2; exit 1; }

cleanup() {
  local pid
  for pid in "${MONITOR_PID:-}" "${HARNESS_PID:-}"; do
    [[ -n "$pid" ]] || continue
    kill -INT "$pid" 2>/dev/null || true
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  [[ -z "${FIFO:-}" ]] || rm -f "$FIFO"
  if [[ -n "${LEAF:-}" && -d "$LEAF" ]]; then
    rmdir "$LEAF" 2>/dev/null || true
  fi
}
trap cleanup EXIT

send_debug() {
  grep -q "DEBUG: Attached Tracepoint sys_enter_sendto" "$LOG" ||
    grep -q "DEBUG: Attached Tracepoint sys_enter_sendmsg" "$LOG"
}

disable_send_attach() {
  local src="$ROOT/crates/assay-monitor/src/loader.rs"
  python3 - "$src" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
old = "attach_send_tracepoint(&mut bpf, r)"
new = 'Err((SendFault::AttachFailed { kernel_lacks_point: false }, "s1b-cell7-disabled".into()))'
t = p.read_text()
if old not in t:
    raise SystemExit("mutation target missing")
p.write_text(t.replace(old, new, 1))
PY
  grep -q 's1b-cell7-disabled' "$src" || fail "mutation marker missing"
  grep -q 'attach_send_tracepoint(&mut bpf' "$src" && fail "send attach call site still present"
  echo "ok: send attach call replaced; caller must restore loader.rs"
}

wait_log() {
  local pat="$1" n
  for ((n = 0; n < 60; n++)); do
    grep -q -- "$pat" "$LOG" && return 0
    if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
      cat "${LOG:-}" >&2 || true
      fail "monitor exited before matching: $pat"
    fi
    sleep 0.5
  done
  cat "${LOG:-}" >&2 || true
  fail "timeout waiting for: $pat"
}

isolate_pid() {
  local cur leaf
  cur="$(awk -F: '$1=="0"{print $3}' "/proc/${1}/cgroup")"
  [[ -n "$cur" ]] || fail "no cgroup v2 path for pid $1"
  leaf="/sys/fs/cgroup${cur}/assay-s1b-$$"
  mkdir -p "$leaf" || fail "mkdir $leaf"
  echo "$1" >"${leaf}/cgroup.procs" || fail "isolate $1 into $leaf"
  echo "$leaf"
}

run_matrix() {
  local expect_send="$1" n=0 hpid="" hc=0 mc=0 tcp p2 p3 b summary
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
  echo GO >"$FIFO"
  wait "$HARNESS_PID" || hc=$?
  HARNESS_PID=""
  [[ "$hc" -eq 0 ]] || fail "harness exit $hc (receiver effect missing)"
  grep -q 'HARNESS_OK' "$HOUT" || fail "harness did not print HARNESS_OK"

  kill -INT "$MONITOR_PID" 2>/dev/null || true
  wait "$MONITOR_PID" || mc=$?
  MONITOR_PID=""
  echo "monitor exit=$mc"

  tcp="$(awk -F= '/^CELL1_TCP_PORT=/{print $2; exit}' "$HOUT")"
  p2="$(awk -F= '/^CELL2_UDP_PORT=/{print $2; exit}' "$HOUT")"
  p3="$(awk -F= '/^CELL3_UDP_PORT=/{print $2; exit}' "$HOUT")"
  [[ -n "$tcp" && -n "$p2" && -n "$p3" ]] || fail "missing bound ports in harness stdout"
  grep -q "CELL_OK 1 accept" "$HOUT" || fail "cell 1 accept missing"
  for b in a2 a3 a4 a5 a6 a7; do
    grep -qi "CELL_OK recv=0x${b}" "$HOUT" || fail "receiver byte 0x${b} missing"
  done
  grep -q "\\[PID ${hpid}\\] connect: 127.0.0.1:${tcp}" "$LOG" \
    || fail "cell 1 connect line missing for 127.0.0.1:${tcp}"

  summary="$(grep 'Send observation:' "$LOG" || true)"
  [[ -n "$summary" ]] || fail "missing Send observation summary"

  if [[ "$expect_send" == "yes" ]]; then
    grep -q "\\[PID ${hpid}\\] sendto: 127.0.0.1:${p2}" "$LOG" \
      || fail "cell 2 sendto endpoint missing"
    grep -q "\\[PID ${hpid}\\] sendmsg: 127.0.0.1:${p3}" "$LOG" \
      || fail "cell 3 sendmsg endpoint missing"
    [[ "$(grep -c "\\[PID ${hpid}\\] sendto:" "$LOG")" -eq 1 &&
      "$(grep -c "\\[PID ${hpid}\\] sendmsg:" "$LOG")" -eq 1 ]] ||
      fail "expected exactly one sendto and one sendmsg endpoint line"
    grep -q 'sendto emitted=1 dropped=0 no_peer=1 non_ip=1; sendmsg emitted=1 dropped=0 no_peer=1 non_ip=1' \
      <<<"$summary" || fail "exact send counts missing: $summary"
  else
    send_debug && fail "send DEBUG attach lines present in attach-disabled run"
    grep -Eq "\\[PID ${hpid}\\] send(to|msg):" "$LOG" &&
      fail "send endpoint lines present in attach-disabled run"
    grep -q 'sendto emitted=0 dropped=0 no_peer=0 non_ip=0; sendmsg emitted=0 dropped=0 no_peer=0 non_ip=0' \
      <<<"$summary" || fail "attach-disabled send stats not all zero: $summary"
  fi

  grep 'Tracepoint ringbuf:' "$LOG" | grep -q 'dropped=0' \
    || fail "tracepoint drop field is not 0"
  python3 -c 'import json,sys; c=json.load(open(sys.argv[1])).get("network_protocol_coverage");
raise SystemExit(c) if c in ("datagram_peer_observed","connect_and_datagram_peer_observed") else print("ok: network_protocol_coverage="+str(c))' "$OH"
  echo "ok: $MODE matrix"
}

case "$MODE" in
  disable-send-attach) disable_send_attach ;;
  positive)
    [[ -x "${HARNESS_BIN:-}" && -x "$ASSAY_BIN" && -f "$ASSAY_EBPF" ]] || fail "missing bin/object"
    run_matrix yes ;;
  attach-disabled)
    [[ -x "${HARNESS_BIN:-}" && -x "$ASSAY_BIN" && -f "$ASSAY_EBPF" ]] || fail "missing bin/object"
    run_matrix no ;;
  cleanup-selftest)
    WORKDIR=$(mktemp -d); FIFO=$WORKDIR/go.fifo; mkfifo "$FIFO"
    sleep 30 & MONITOR_PID=$!; sleep 30 & HARNESS_PID=$!
    LEAF=$(mktemp -d); echo "$MONITOR_PID $HARNESS_PID $FIFO $LEAF"
    fail "injected failure" ;;
  *) fail "usage: $0 positive|attach-disabled|disable-send-attach|cleanup-selftest" ;;
esac
