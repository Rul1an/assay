#!/usr/bin/env bash
# S1b driver: cgroup/PID isolate, start monitor, GO, assert effects + telemetry.
set -euo pipefail
S1B_OWNED_WORKDIR=
S1B_OWNED_ID=
S1B_OWNED_SAVED_PWD=

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
MODE="${1:-}"
ASSAY_BIN="${ASSAY_BIN:-$ROOT/target/release/assay}"
ASSAY_EBPF="${ASSAY_EBPF:-$ROOT/target/assay-ebpf.o}"
HARNESS_BIN="${HARNESS_BIN:-}"
WORKDIR="${WORKDIR:-}"
# Bound: C GO_FIFO_TIMEOUT_MS >= WAIT_LOG_MAX_BEFORE_GO * WAIT_LOG_ITERS * WAIT_LOG_SLEEP_S * 1000 + GO_TIMEOUT_MARGIN_MS
WAIT_LOG_ITERS=60
WAIT_LOG_SLEEP_S=0.5
WAIT_LOG_MAX_BEFORE_GO=4
GO_TIMEOUT_MARGIN_MS=30000
DIAG_MAX_LINES=200
MUTATION_OLD='attach_send_tracepoint(&mut bpf, r)'
MUTATION_NEW='Err((SendFault::AttachFailed { kernel_lacks_point: false }, "s1b-cell7-disabled".to_string()))'

emit_matrix_diagnostics() {
  local f path
  for f in LOG HOUT; do
    path="${!f:-}"
    [[ -n "$path" && -f "$path" ]] || continue
    echo "----- begin $f -----" >&2
    sed -n "1,${DIAG_MAX_LINES}p" "$path" >&2 || true
    echo "----- end $f -----" >&2
  done
}

fail() {
  echo "FAIL: $*" >&2
  emit_matrix_diagnostics
  exit 1
}
[[ "$WAIT_LOG_MAX_BEFORE_GO" -ge 1 && "$GO_TIMEOUT_MARGIN_MS" -ge 0 ]] \
  || fail "invalid GO bound constants"

REAP_POLL_MAX="${REAP_POLL_MAX:-10}"
REAP_POLL_SLEEP="${REAP_POLL_SLEEP:-0.1}"

re_escape() {
  printf '%s' "$1" | sed -e 's/[][\\.^$*+?(){}|]/\\&/g'
}

endpoint_line_ok() {
  local file="$1" pid="$2" op="$3" ip="$4" port="$5"
  grep -Eq "^$(re_escape "[PID ${pid}] ${op}: ${ip}:${port}")$" "$file"
}

harness_ok() {
  grep -qx 'HARNESS_OK' "$1"
}

ringbuf_drops_ok() {
  local log="$1" line found=0
  while IFS= read -r line; do
    found=1
    [[ "$line" =~ (^|[[:space:]])dropped=0([^[:alnum:]]|$) ]] || return 1
  done < <(grep 'Tracepoint ringbuf:' "$log" || true)
  [[ "$found" -eq 1 ]]
}

# Controlled SIGINT: monitor_next catches ctrl_c, breaks the select, returns OK (0).
# 130 would mean the process died from the signal instead of that path.
monitor_shutdown_ok() {
  [[ "$1" -eq 0 ]]
}

# Must match assay-cli format_send_observation_summary exactly (full line).
send_observation_ok() {
  local haystack="$1"
  shift
  local want
  want="$(printf '  • Send observation:   sendto emitted=%s dropped=%s no_peer=%s non_ip=%s; sendmsg emitted=%s dropped=%s no_peer=%s non_ip=%s' "$@")"
  grep -qxF -- "$want" <<<"$haystack"
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

s1b_stat_id() {
  local target="$1"
  case "$(uname -s)" in
    Linux) stat -c '%d:%i' "$target" ;;
    *) stat -f '%d:%i' "$target" ;;
  esac
}

s1b_release_owned_cwd() {
  if [[ -n "${S1B_OWNED_SAVED_PWD:-}" && -d "$S1B_OWNED_SAVED_PWD" ]]; then
    cd "$S1B_OWNED_SAVED_PWD" || cd /
  else
    cd / || true
  fi
}

s1b_path_is_owned_object() {
  local wd="$1" path_id
  [[ -n "${S1B_OWNED_ID:-}" && -n "$wd" ]] || return 1
  path_id=$(s1b_stat_id "$wd" 2>/dev/null) || return 1
  [[ "$path_id" == "$S1B_OWNED_ID" ]]
}

s1b_owned_workdir() {
  local wd="$1" cwd_id
  [[ -n "$wd" && "$wd" == "${S1B_OWNED_WORKDIR:-}" && -n "${S1B_OWNED_ID:-}" ]] \
    || return 1
  cwd_id=$(s1b_stat_id . 2>/dev/null) || return 1
  [[ "$cwd_id" == "$S1B_OWNED_ID" ]]
}

create_owned_workdir() {
  local rt="${RUNNER_TEMP:-}" wd="${WORKDIR:-}" base parent
  [[ -n "$rt" ]] || fail "RUNNER_TEMP required for S1b workdir ownership"
  [[ "$rt" == /* && -d "$rt" ]] || fail "invalid RUNNER_TEMP=$rt"
  case "$rt" in
    *'/../'*|*/..|..) fail "refusing RUNNER_TEMP with traversal: $rt" ;;
  esac
  if [[ -z "$wd" ]]; then
    wd=$(mktemp -d "$rt/s1b-${MODE:-run}-XXXXXX") || fail "mktemp owned WORKDIR"
  else
    [[ "$wd" == /* ]] || fail "refusing relative WORKDIR=$wd"
    case "$wd" in
      *'/../'*|*/..|..) fail "refusing WORKDIR with traversal: $wd" ;;
    esac
    base="${wd##*/}"
    parent="${wd%/*}"
    [[ "$base" == s1b-* && "$parent" == "$rt" ]] \
      || fail "refusing WORKDIR outside RUNNER_TEMP namespace: $wd"
    [[ ! -e "$wd" && ! -L "$wd" ]] || fail "refusing existing WORKDIR=$wd"
    mkdir "$wd" || fail "mkdir WORKDIR=$wd"
  fi
  WORKDIR="$wd"
  S1B_OWNED_WORKDIR="$wd"
  S1B_OWNED_ID=$(s1b_stat_id "$wd") || fail "stat owned WORKDIR=$wd"
  S1B_OWNED_SAVED_PWD=$PWD
  cd "$wd" || fail "enter owned WORKDIR=$wd"
}

remove_owned_workdir() {
  local wd="${1:-}" contents_residue=0
  [[ -n "$wd" ]] || return 0
  [[ -n "${S1B_OWNED_WORKDIR:-}" ]] || return 0
  if ! s1b_owned_workdir "$wd"; then
    s1b_release_owned_cwd
    printf 'S1B_WORKDIR_RESIDUE path=%s reason=lost_object\n' "$wd" >&2
    return 1
  fi
  find . -mindepth 1 -maxdepth 1 -exec rm -rf -- {} + || contents_residue=1
  if find . -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
    contents_residue=1
  fi
  s1b_release_owned_cwd
  if [[ "$contents_residue" -ne 0 ]]; then
    printf 'S1B_WORKDIR_RESIDUE path=%s reason=contents\n' "$wd" >&2
    return 1
  fi
  if ! s1b_path_is_owned_object "$wd"; then
    printf 'S1B_WORKDIR_RESIDUE path=%s reason=path_rebound\n' "$wd" >&2
    return 1
  fi
  printf 'S1B_WORKDIR_RETAINED path=%s\n' "$wd"
}

cleanup() {
  local incoming_status="${1:-0}" leaf_residue=0 workdir_residue=0 pid
  for pid in "${MONITOR_PID:-}" "${HARNESS_PID:-}"; do
    reap_pid "$pid"
  done
  if [[ -n "${LEAF:-}" && -d "$LEAF" ]]; then
    if ! rmdir "$LEAF" 2>/dev/null; then
      printf 'S1B_LEAF_RESIDUE path=%s\n' "$LEAF" >&2
      leaf_residue=1
    fi
  fi
  remove_owned_workdir "${WORKDIR:-}" || workdir_residue=1
  [[ "$incoming_status" -eq 0 ]] || return "$incoming_status"
  [[ "$leaf_residue" -eq 0 && "$workdir_residue" -eq 0 ]]
}

cleanup_on_exit() {
  local incoming_status="$1" cleanup_status
  trap - EXIT
  set +e
  cleanup "$incoming_status"
  cleanup_status=$?
  exit "$cleanup_status"
}

trap 'cleanup_on_exit "$?"' EXIT
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
  for ((n = 0; n < WAIT_LOG_ITERS; n++)); do
    grep -q -- "$pat" "$LOG" && return 0
    if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
      cat "${LOG:-}" >&2 || true
      fail "monitor exited before matching: $pat"
    fi
    sleep "$WAIT_LOG_SLEEP_S"
  done
  cat "${LOG:-}" >&2 || true
  fail "timeout waiting for: $pat"
}

wait_endpoint() {
  local pid="$1" op="$2" ip="$3" port="$4" n
  for ((n = 0; n < WAIT_LOG_ITERS; n++)); do
    endpoint_line_ok "$LOG" "$pid" "$op" "$ip" "$port" && return 0
    if ! kill -0 "$MONITOR_PID" 2>/dev/null; then
      fail "monitor exited before matching: $op $ip:$port"
    fi
    sleep "$WAIT_LOG_SLEEP_S"
  done
  fail "timeout waiting for: $op $ip:$port"
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

prepare_matrix_artifacts() {
  FIFO=go.fifo
  LOG=monitor.log
  HOUT=harness.out
  OH=observation-health.json
  mkfifo "$FIFO"
}

run_matrix() {
  local expect_send="$1" n=0 hpid="" hc=0 mc=0 p2 p3
  create_owned_workdir
  prepare_matrix_artifacts

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
    selftest_rt=$(mktemp -d)
    RUNNER_TEMP="$selftest_rt"
    WORKDIR="$RUNNER_TEMP/s1b-cleanup-selftest-$$"
    create_owned_workdir
    s1b_owned_workdir "$WORKDIR" || fail "cleanup-selftest WORKDIR is not an owned S1b path: $WORKDIR"
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
    wd=$WORKDIR
    [[ -d "$wd" ]] || fail "retained WORKDIR root missing $wd"
    [[ -z "$(find "$wd" -mindepth 1 -maxdepth 1 -print -quit)" ]] \
      || fail "retained WORKDIR contents $wd"
    rmdir "$wd" || fail "consume retained WORKDIR=$wd"
    rmdir "$selftest_rt" || fail "consume cleanup selftest RUNNER_TEMP=$selftest_rt"
    bad=$(mktemp -d)
    printf 'keep\n' >"$bad/keep"
    if s1b_owned_workdir "$bad"; then
      fail "unowned path was accepted: $bad"
    fi
    set +e
    ( remove_owned_workdir "$bad" )
    rec=$?
    set -e
    [[ "$rec" -ne 0 ]] || fail "remove_owned_workdir deleted unowned path $bad"
    [[ -f "$bad/keep" ]] || fail "unowned WORKDIR was deleted: $bad"
    rm -rf "$bad"
    WORKDIR="" S1B_OWNED_WORKDIR="" S1B_OWNED_ID="" S1B_OWNED_SAVED_PWD=""
    echo "ok: cleanup-selftest" ;;
  cleanup-leaf-status-selftest)
    [[ -n "${2:-}" && -d "$2" ]] || fail "cleanup leaf selftest requires an existing leaf directory"
    [[ -n "$(find "$2" -mindepth 1 -maxdepth 1 -print -quit)" ]] \
      || fail "cleanup leaf selftest requires a non-empty leaf directory"
    [[ "${3:-}" =~ ^([0-9]|[1-9][0-9]|1[0-9][0-9]|2[0-4][0-9]|25[0-5])$ ]] \
      || fail "cleanup leaf selftest requires an exit status in 0..255"
    LEAF="$2"
    WORKDIR=""
    create_owned_workdir
    printf 'SELFTEST_WORKDIR=%s\n' "$WORKDIR"
    exit "$3" ;;
  workdir-create-selftest)
    create_owned_workdir
    printf 'SELFTEST_WORKDIR=%s\n' "$WORKDIR"
    printf 'owned\n' >"$WORKDIR/selftest-owned"
    foreign="$RUNNER_TEMP/s1b-foreign-$$"
    [[ ! -e "$foreign" ]] || fail "foreign selftest path exists: $foreign"
    mkdir "$foreign"
    printf 'keep\n' >"$foreign/keep"
    if s1b_owned_workdir "$foreign"; then
      fail "foreign S1b sibling considered owned: $foreign"
    fi
    printf 'SELFTEST_FOREIGN_WORKDIR=%s\n' "$foreign"
    exit 0 ;;
  workdir-rebind-selftest)
    create_owned_workdir
    owned=$WORKDIR
    moved="${WORKDIR}-moved"
    [[ ! -e "$moved" ]] || fail "rebind target exists: $moved"
    FIFO="$WORKDIR/go.fifo"
    mkfifo "$FIFO"
    printf 'owned\n' >"$WORKDIR/selftest-owned"
    mv "$owned" "$moved"
    mkdir "$owned"
    printf 'foreign\n' >"$owned/foreign"
    mkfifo "$owned/go.fifo"
    printf 'SELFTEST_WORKDIR=%s\n' "$owned"
    printf 'SELFTEST_REBOUND_WORKDIR=%s\n' "$moved"
    exit 0 ;;
  workdir-startup-rebind-selftest)
    create_owned_workdir
    owned=$WORKDIR
    moved="${WORKDIR}-moved"
    [[ ! -e "$moved" ]] || fail "startup rebind target exists: $moved"
    mv "$owned" "$moved"
    mkdir "$owned"
    for artifact in go.fifo monitor.log harness.out observation-health.json; do
      printf 'foreign:%s\n' "$artifact" >"$owned/$artifact"
    done
    prepare_matrix_artifacts
    printf 'SELFTEST_WORKDIR=%s\n' "$owned"
    printf 'SELFTEST_REBOUND_WORKDIR=%s\n' "$moved"
    exit 0 ;;
  coverage-gate) coverage_gate "${2:?coverage-gate requires a JSON path}" ;;
  mutation-selftest) mutation_selftest ;;
  endpoint-line-selftest)
    [[ -n "${2:-}" && -n "${3:-}" && -n "${4:-}" && -n "${5:-}" && -n "${6:-}" ]] \
      || fail "endpoint-line-selftest requires LOG PID OP IP PORT"
    endpoint_line_ok "$2" "$3" "$4" "$5" "$6" || fail "endpoint line not ok"
    echo "ok: endpoint-line-selftest" ;;
  harness-ok-selftest)
    [[ -n "${2:-}" ]] || fail "harness-ok-selftest requires a file"
    harness_ok "$2" || fail "harness did not print HARNESS_OK"
    echo "ok: harness-ok-selftest" ;;
  ringbuf-drop-selftest)
    [[ -n "${2:-}" ]] || fail "ringbuf-drop-selftest requires a log"
    ringbuf_drops_ok "$2" || fail "tracepoint drop field is not 0"
    echo "ok: ringbuf-drop-selftest" ;;
  send-observation-selftest)
    [[ -n "${2:-}" && -n "${10:-}" ]] || fail "send-observation-selftest requires LOG and 8 counts"
    send_observation_ok "$(cat "$2")" "$3" "$4" "$5" "$6" "$7" "$8" "$9" "${10}" \
      || fail "send observation summary not exact"
    echo "ok: send-observation-selftest" ;;
  monitor-shutdown-selftest)
    [[ -n "${2:-}" ]] || fail "monitor-shutdown-selftest requires an exit code"
    monitor_shutdown_ok "$2" || fail "exit $2 is not controlled SIGINT shutdown"
    echo "ok: monitor-shutdown-selftest" ;;
  diagnostics-selftest)
    LOG="${2:?}"
    HOUT="${3:?}"
    fail "diagnostics-selftest" ;;
  *) fail "usage: $0 positive|attach-disabled|disable-send-attach|cleanup-selftest|cleanup-leaf-status-selftest|workdir-create-selftest|workdir-rebind-selftest|workdir-startup-rebind-selftest|coverage-gate|mutation-selftest|endpoint-line-selftest|harness-ok-selftest|ringbuf-drop-selftest|send-observation-selftest|monitor-shutdown-selftest|diagnostics-selftest" ;;
esac
