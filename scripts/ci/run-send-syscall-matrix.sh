#!/usr/bin/env bash
# S1b driver: cgroup/PID isolate, start monitor, GO, assert effects + telemetry.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODE="${1:-}"
ASSAY_BIN="${ASSAY_BIN:-$ROOT/target/release/assay}"
ASSAY_EBPF="${ASSAY_EBPF:-$ROOT/target/assay-ebpf.o}"
HARNESS_BIN="${HARNESS_BIN:-}"
WORKDIR="${WORKDIR:-${RUNNER_TEMP:-/tmp}/s1b-send-matrix}"
MUTATION_OLD='attach_send_tracepoint(&mut bpf, r)'
MUTATION_NEW='Err((SendFault::AttachFailed { kernel_lacks_point: false }, "s1b-cell7-disabled".to_string()))'

fail() { echo "FAIL: $*" >&2; exit 1; }

REAP_POLL_MAX="${REAP_POLL_MAX:-10}"
REAP_POLL_SLEEP="${REAP_POLL_SLEEP:-0.1}"

re_escape() {
  printf '%s' "$1" | sed -e 's/[][\\.^$*+?(){}|]/\\&/g'
}

endpoint_line_ok() {
  local file="$1" ip="$2" port="$3"
  grep -Eq "$(re_escape "$ip"):$(re_escape "$port")$" "$file"
}

harness_ok() {
  grep -qx 'HARNESS_OK' "$1"
}

ringbuf_drops_ok() {
  local log="$1" line found=0
  while IFS= read -r line; do
    found=1
    [[ "$line" == *dropped=0* ]] || return 1
  done < <(grep 'Tracepoint ringbuf:' "$log" || true)
  [[ "$found" -eq 1 ]]
}

wait_pid_gone() {
  local pid="$1" n=0
  while (( n < REAP_POLL_MAX )); do
    kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null || true; return 0; }
    sleep "$REAP_POLL_SLEEP"
    n=$((n + 1))
  done
  return 1
}

reap_pid() {
  local pid="${1:-}"
  [[ -n "$pid" ]] || return 0
  kill -0 "$pid" 2>/dev/null || { wait "$pid" 2>/dev/null || true; return 0; }
  kill -INT "$pid" 2>/dev/null || true
  wait_pid_gone "$pid" && return 0
  kill -TERM "$pid" 2>/dev/null || true
  wait_pid_gone "$pid" && return 0
  kill -KILL "$pid" 2>/dev/null || true
  kill -KILL -- "-$pid" 2>/dev/null || true
  wait_pid_gone "$pid" && return 0
  wait "$pid" 2>/dev/null || true
  kill -0 "$pid" 2>/dev/null && fail "pid $pid still alive after SIGKILL bound; hang is not clean"
}

cleanup() {
  local pid
  for pid in "${MONITOR_PID:-}" "${HARNESS_PID:-}"; do
    reap_pid "$pid"
  done
  [[ -z "${FIFO:-}" ]] || rm -f "$FIFO"
  if [[ -n "${LEAF:-}" && -d "$LEAF" ]]; then
    rmdir "$LEAF" 2>/dev/null || true
  fi
}
trap cleanup EXIT
trap 'exit 143' TERM
trap 'exit 130' INT

send_debug() {
  grep -q "DEBUG: Attached Tracepoint sys_enter_sendto" "$LOG" ||
    grep -q "DEBUG: Attached Tracepoint sys_enter_sendmsg" "$LOG"
}

mutate_loader() {
  local src="$1"
  python3 - "$src" "$MUTATION_OLD" "$MUTATION_NEW" <<'PY'
from pathlib import Path
import sys
p = Path(sys.argv[1])
old, new = sys.argv[2], sys.argv[3]
t = p.read_text()
n = t.count(old)
if n != 1:
    raise SystemExit(f"mutation target count {n}, want 1")
p.write_text(t.replace(old, new, 1))
PY
  grep -q 's1b-cell7-disabled' "$src" || fail "mutation marker missing"
  if grep -q 'attach_send_tracepoint(&mut bpf' "$src"; then
    fail "send attach call site still present"
  fi
}

disable_send_attach() {
  mutate_loader "$ROOT/crates/assay-monitor/src/loader.rs"
  echo "ok: send attach call replaced; caller must restore loader.rs"
}

write_go() {
  python3 - "$FIFO" <<'PY'
import errno
import os
import sys
import time

path = sys.argv[1]
deadline = time.time() + 10
while time.time() < deadline:
    try:
        fd = os.open(path, os.O_WRONLY | os.O_NONBLOCK)
        os.write(fd, b"GO\n")
        os.close(fd)
        sys.exit(0)
    except OSError as e:
        if e.errno not in (errno.ENXIO, errno.EAGAIN, errno.EWOULDBLOCK):
            raise
        time.sleep(0.1)
raise SystemExit("GO fifo write timeout")
PY
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
  write_go
  wait "$HARNESS_PID" || hc=$?
  HARNESS_PID=""
  [[ "$hc" -eq 0 ]] || fail "harness exit $hc (receiver effect missing)"
  harness_ok "$HOUT" || fail "harness did not print HARNESS_OK"

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
  endpoint_line_ok "$LOG" "127.0.0.1" "$tcp" \
    || fail "cell 1 connect line missing for 127.0.0.1:${tcp}"

  summary="$(grep 'Send observation:' "$LOG" || true)"
  [[ -n "$summary" ]] || fail "missing Send observation summary"

  if [[ "$expect_send" == "yes" ]]; then
    endpoint_line_ok "$LOG" "127.0.0.1" "$p2" \
      || fail "cell 2 sendto endpoint missing"
    endpoint_line_ok "$LOG" "127.0.0.1" "$p3" \
      || fail "cell 3 sendmsg endpoint missing"
    [[ "$(grep -c "\\[PID ${hpid}\\] sendto:" "$LOG")" -eq 1 &&
      "$(grep -c "\\[PID ${hpid}\\] sendmsg:" "$LOG")" -eq 1 ]] ||
      fail "expected exactly one sendto and one sendmsg endpoint line"
    grep -q 'sendto emitted=1 dropped=0 no_peer=1 non_ip=1; sendmsg emitted=1 dropped=0 no_peer=1 non_ip=1' \
      <<<"$summary" || fail "exact send counts missing: $summary"
  else
    if send_debug; then
      fail "send DEBUG attach lines present in attach-disabled run"
    fi
    if grep -Eq "\\[PID ${hpid}\\] send(to|msg):" "$LOG"; then
      fail "send endpoint lines present in attach-disabled run"
    fi
    grep -q 'sendto emitted=0 dropped=0 no_peer=0 non_ip=0; sendmsg emitted=0 dropped=0 no_peer=0 non_ip=0' \
      <<<"$summary" || fail "attach-disabled send stats not all zero: $summary"
  fi

  ringbuf_drops_ok "$LOG" || fail "tracepoint drop field is not 0"
  coverage_gate "$OH"
  echo "ok: $MODE matrix"
}

coverage_gate() {
  python3 - "$1" <<'PY'
import json
import sys

c = json.load(open(sys.argv[1])).get("network_protocol_coverage")
if c in ("connect_only", "absent"):
    print("ok: network_protocol_coverage=" + str(c))
    sys.exit(0)
raise SystemExit("unexpected network_protocol_coverage=" + repr(c))
PY
}

mutation_selftest() {
  local src="$ROOT/crates/assay-monitor/src/loader.rs"
  local orig work rustdir
  orig=$(mktemp)
  work=$(mktemp)
  rustdir=$(mktemp -d)
  cp "$src" "$orig"
  cp "$src" "$work"
  python3 - "$orig" "$MUTATION_OLD" <<'PY'
import sys
from pathlib import Path
n = Path(sys.argv[1]).read_text().count(sys.argv[2])
raise SystemExit(0 if n == 1 else f"original target count {n}, want 1")
PY
  mutate_loader "$work"
  grep -q 's1b-cell7-disabled".to_string()' "$work" || fail "to_string mutant missing"
  cp "$orig" "$work"
  cmp -s "$orig" "$work" || fail "restore did not put original bytes back"
  cat >"$rustdir/into.rs" <<'RS'
#![allow(dead_code)]
fn main() {
    enum SendFault {
        AttachFailed { kernel_lacks_point: bool },
    }
    struct TracePointLink;
    match Err((
        SendFault::AttachFailed {
            kernel_lacks_point: false,
        },
        "s1b-cell7-disabled".into(),
    )) {
        Ok(link) => {
            let _: TracePointLink = link;
        }
        Err((fault, detail)) => {
            let _ = fault;
            eprintln!("{}", detail);
        }
    }
}
RS
  cat >"$rustdir/to_string.rs" <<'RS'
#![allow(dead_code)]
fn main() {
    enum SendFault {
        AttachFailed { kernel_lacks_point: bool },
    }
    struct TracePointLink;
    match Err((
        SendFault::AttachFailed {
            kernel_lacks_point: false,
        },
        "s1b-cell7-disabled".to_string(),
    )) {
        Ok(link) => {
            let _: TracePointLink = link;
        }
        Err((fault, detail)) => {
            let _ = fault;
            eprintln!("{}", detail);
        }
    }
}
RS
  set +e
  rustc --edition 2021 -o "$rustdir/into" "$rustdir/into.rs" 2>"$rustdir/into.err"
  into_ec=$?
  set -e
  [[ "$into_ec" -ne 0 ]] || fail ".into() mutant type-checked; expected failure"
  grep -q . "$rustdir/into.err" || fail ".into() rustc produced no error"
  rustc --edition 2021 -o "$rustdir/to_string" "$rustdir/to_string.rs" \
    || fail ".to_string() mutant failed to type-check"
  rm -f "$orig" "$work"
  rm -rf "$rustdir"
  echo "ok: mutation uniqueness, restore, into-fails, to_string-compiles"
}

case "$MODE" in
  disable-send-attach) disable_send_attach ;;
  positive)
    [[ -x "${HARNESS_BIN:-}" && -x "$ASSAY_BIN" && -f "$ASSAY_EBPF" ]] || fail "missing bin/object"
    run_matrix yes ;;
  attach-disabled)
    [[ -x "${HARNESS_BIN:-}" && -x "$ASSAY_BIN" && -f "$ASSAY_EBPF" ]] || fail "missing bin/object"
    python3 -c 'import pathlib,sys; sys.exit(0 if b"s1b-cell7-disabled" in pathlib.Path(sys.argv[1]).read_bytes() else 1)' \
      "$ASSAY_BIN" || fail "ASSAY_BIN is not the mutated rebuild (missing s1b-cell7-disabled)"
    run_matrix no ;;
  cleanup-selftest)
    WORKDIR=$(mktemp -d)
    FIFO=$WORKDIR/go.fifo
    mkfifo "$FIFO"
    bash -c 'trap "" TERM INT; echo ready; while :; do sleep 1; done' >/dev/null &
    MONITOR_PID=$!
    bash -c 'trap "" TERM INT; echo ready; while :; do sleep 1; done' >/dev/null &
    HARNESS_PID=$!
    sleep 0.2
    if ! kill -0 "$MONITOR_PID" 2>/dev/null || ! kill -0 "$HARNESS_PID" 2>/dev/null; then
      fail "TERM-ignoring children died before cleanup"
    fi
    LEAF=$(mktemp -d)
    mp=$MONITOR_PID hp=$HARNESS_PID fifo=$FIFO leaf=$LEAF
    start=$(date +%s)
    cleanup
    elapsed=$(( $(date +%s) - start ))
    MONITOR_PID="" HARNESS_PID="" FIFO="" LEAF=""
    (( elapsed < 8 )) || fail "cleanup hung (${elapsed}s); hang is not clean"
    if kill -0 "$mp" 2>/dev/null; then
      fail "monitor pid $mp still alive"
    fi
    if kill -0 "$hp" 2>/dev/null; then
      fail "harness pid $hp still alive"
    fi
    [[ ! -e "$fifo" ]] || fail "FIFO leftover $fifo"
    [[ ! -d "$leaf" ]] || fail "leaf leftover $leaf"
    echo "ok: cleanup-selftest" ;;
  coverage-gate) coverage_gate "${2:?coverage-gate requires a JSON path}" ;;
  mutation-selftest) mutation_selftest ;;
  endpoint-line-selftest)
    [[ -n "${2:-}" && -n "${3:-}" && -n "${4:-}" ]] || fail "endpoint-line-selftest requires LOG IP PORT"
    endpoint_line_ok "$2" "$3" "$4" || fail "endpoint line not ok"
    echo "ok: endpoint-line-selftest" ;;
  harness-ok-selftest)
    [[ -n "${2:-}" ]] || fail "harness-ok-selftest requires a file"
    harness_ok "$2" || fail "harness did not print HARNESS_OK"
    echo "ok: harness-ok-selftest" ;;
  ringbuf-drop-selftest)
    [[ -n "${2:-}" ]] || fail "ringbuf-drop-selftest requires a log"
    ringbuf_drops_ok "$2" || fail "tracepoint drop field is not 0"
    echo "ok: ringbuf-drop-selftest" ;;
  *) fail "usage: $0 positive|attach-disabled|disable-send-attach|cleanup-selftest|coverage-gate|mutation-selftest|endpoint-line-selftest|harness-ok-selftest|ringbuf-drop-selftest" ;;
esac
