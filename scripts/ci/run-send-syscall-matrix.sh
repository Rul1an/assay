#!/usr/bin/env bash
# S1b driver: cgroup/PID isolate, start monitor, GO, assert effects + telemetry.
set -euo pipefail
S1B_HYGIENE=

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
  local p="$1"
  case "$(uname -s)" in
    Linux) stat -c '%d:%i' "$p" ;;
    *) stat -f '%d:%i' "$p" ;;
  esac
}

s1b_path_is_owned_object() {
  local wd="$1" path_id
  [[ -n "${S1B_OWNED_ID:-}" && -n "$wd" ]] || return 1
  path_id=$(s1b_stat_id "$wd") || return 1
  [[ "$path_id" == "$S1B_OWNED_ID" ]]
}

s1b_release_owned_cwd() {
  if [[ -n "${S1B_OWNED_SAVED_PWD:-}" && -d "$S1B_OWNED_SAVED_PWD" ]]; then
    cd "$S1B_OWNED_SAVED_PWD" || cd /
  else
    cd / || true
  fi
}

record_owned_workdir() {
  local wd="$1"
  if [[ -n "${S1B_OWNED_SAVED_PWD:-}" ]]; then
    cd "$S1B_OWNED_SAVED_PWD" 2>/dev/null || cd /
  else
    S1B_OWNED_SAVED_PWD=$PWD
  fi
  S1B_OWNED_WORKDIR="$wd"
  S1B_OWNED_ID=
  if [[ -d "$wd" ]]; then
    S1B_OWNED_ID=$(s1b_stat_id "$wd") || fail "stat owned WORKDIR $wd"
    cd "$wd" || fail "cd owned WORKDIR $wd"
  fi
}

s1b_hygiene_track() {
  local p="$1"
  [[ -n "$p" ]] || return 0
  S1B_HYGIENE+="${S1B_HYGIENE:+$'\n'}$p"
  echo "S1B_HYGIENE_TRACK=$p"
}

s1b_hygiene_sweep() {
  local p
  [[ -n "${S1B_HYGIENE:-}" ]] || return 0
  while IFS= read -r p; do
    [[ -n "$p" ]] || continue
    rm -rf "$p"
  done <<<"$S1B_HYGIENE"
}

create_owned_workdir() {
  local wd base dir
  if [[ -z "${WORKDIR:-}" ]]; then
    wd="$(mktemp -d "${RUNNER_TEMP:-/tmp}/s1b-XXXXXX")" || fail "mktemp owned WORKDIR"
    WORKDIR="$wd"
    record_owned_workdir "$WORKDIR"
    return 0
  fi
  wd="$WORKDIR"
  case "$wd" in
    *'/../'*|*/..|..)
      echo "FAIL: refusing WORKDIR with traversal: $wd" >&2
      return 1
      ;;
  esac
  [[ "$wd" == /* ]] || { echo "FAIL: refusing relative WORKDIR=$wd" >&2; return 1; }
  base="${wd##*/}"
  dir="${wd%/*}"
  [[ -n "$dir" ]] || dir=/
  if [[ "$base" != s1b-* ]] || { [[ "$dir" != /tmp ]] && [[ -z "${RUNNER_TEMP:-}" || "$dir" != "$RUNNER_TEMP" ]]; }; then
    echo "FAIL: refusing WORKDIR outside namespace: $wd" >&2
    return 1
  fi
  if [[ -e "$wd" ]]; then
    echo "FAIL: refusing existing WORKDIR=$wd" >&2
    return 1
  fi
  mkdir "$wd" || fail "mkdir WORKDIR=$wd"
  record_owned_workdir "$WORKDIR"
}
s1b_owned_workdir() {
  local wd="$1"
  [[ -n "$wd" && "$wd" == /* ]] || return 1
  case "$wd" in
    *'/../'*|*/..|..) return 1 ;;
  esac
  [[ -n "${S1B_OWNED_WORKDIR:-}" && "$wd" == "$S1B_OWNED_WORKDIR" ]]
}

remove_owned_workdir() {
  local wd="${1:-}" cwd_id
  [[ -n "$wd" && -d "$wd" ]] || return 0
  if ! s1b_owned_workdir "$wd"; then
    if [[ "${S1B_CLEANUP:-}" == 1 ]]; then
      echo "FAIL: refusing to remove unowned WORKDIR=$wd" >&2
      CLEANUP_LEAF_RC=1
      return 1
    fi
    fail "refusing to remove unowned WORKDIR=$wd"
  fi
  cwd_id=$(s1b_stat_id .)
  if [[ -z "${S1B_OWNED_ID:-}" || "$cwd_id" != "$S1B_OWNED_ID" ]]; then
    echo "FAIL: lost owned WORKDIR object handle" >&2
    CLEANUP_LEAF_RC=1
    if [[ "${S1B_CLEANUP:-}" == 1 ]]; then
      return 1
    fi
    fail "lost owned WORKDIR object handle"
  fi
  find . -mindepth 1 -maxdepth 1 -exec rm -rf {} +
  if s1b_path_is_owned_object "$wd"; then
    s1b_release_owned_cwd
    rmdir "$wd" 2>/dev/null || true
    if [[ -e "$wd" ]]; then
      if [[ "${S1B_CLEANUP:-}" == 1 ]]; then
        echo "FAIL: WORKDIR leftover $wd" >&2
        CLEANUP_LEAF_RC=1
        return 1
      fi
      fail "WORKDIR leftover $wd"
    fi
  else
    echo "FAIL: WORKDIR pathname does not name the owned object: $wd" >&2
    CLEANUP_LEAF_RC=1
    echo "FAIL: owned WORKDIR object leftover after rebind" >&2
    s1b_release_owned_cwd
    if [[ "${S1B_CLEANUP:-}" == 1 ]]; then
      return 1
    fi
    fail "WORKDIR pathname does not name the owned object: $wd"
  fi
}

cleanup_work() {
  local pid
  CLEANUP_LEAF_RC=0
  S1B_CLEANUP=1
  for pid in "${MONITOR_PID:-}" "${HARNESS_PID:-}"; do
    reap_pid "$pid"
  done
  [[ -z "${FIFO:-}" ]] || rm -f "$FIFO"
  if [[ -n "${LEAF:-}" && -d "$LEAF" ]]; then
    if ! rmdir "$LEAF" 2>/dev/null; then
      echo "FAIL: unremovable LEAF=$LEAF" >&2
      CLEANUP_LEAF_RC=1
    fi
  fi
  remove_owned_workdir "${WORKDIR:-}"
  S1B_CLEANUP=
  return "$CLEANUP_LEAF_RC"
}

cleanup() {
  cleanup_work
}

on_exit() {
  local rc="$1"
  trap - EXIT
  set +e
  cleanup_work
  s1b_release_owned_cwd
  s1b_hygiene_sweep
  set -e
  if (( rc != 0 )); then
    exit "$rc"
  fi
  exit "${CLEANUP_LEAF_RC:-0}"
}
trap 'on_exit $?' EXIT
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

run_matrix() {
  local expect_send="$1" n=0 hpid="" hc=0 mc=0 p2 p3
  create_owned_workdir || fail "owned WORKDIR create failed"
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
    WORKDIR=$(mktemp -d /tmp/s1b-cleanup-selftest-XXXXXX)
    s1b_hygiene_track "$WORKDIR"
    record_owned_workdir "$WORKDIR"
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
    s1b_hygiene_track "$LEAF"
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
    [[ ! -e "$wd" ]] || fail "WORKDIR leftover $wd"
    bad=$(mktemp -d)
    s1b_hygiene_track "$bad"
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
    WORKDIR=""
    echo "ok: cleanup-selftest" ;;
  cleanup-collision-selftest)
    assigned=$(mktemp -d /tmp/s1b-XXXXXX)
    s1b_hygiene_track "$assigned"
    WORKDIR=$assigned
    record_owned_workdir "$WORKDIR"
    sib=$(mktemp -d /tmp/s1b-XXXXXX)
    s1b_hygiene_track "$sib"
    printf 'keep\n' >"$sib/keep"
    if s1b_owned_workdir "$sib"; then
      fail "prefix sibling considered owned: $sib"
    fi
    set +e
    ( remove_owned_workdir "$sib" )
    rec=$?
    set -e
    [[ "$rec" -ne 0 ]] || fail "prefix sibling was deleted"
    [[ -f "$sib/keep" ]] || fail "prefix sibling keep missing: $sib"
    rt=$(mktemp -d)
    s1b_hygiene_track "$rt"
    sib2=$(mktemp -d "$rt/s1b-XXXXXX")
    s1b_hygiene_track "$sib2"
    printf 'keep\n' >"$sib2/keep"
    RUNNER_TEMP=$rt
    if s1b_owned_workdir "$sib2"; then
      fail "RUNNER_TEMP prefix sibling considered owned: $sib2"
    fi
    set +e
    ( remove_owned_workdir "$sib2" )
    rec2=$?
    set -e
    [[ "$rec2" -ne 0 ]] || fail "RUNNER_TEMP prefix sibling was deleted"
    [[ -f "$sib2/keep" ]] || fail "RUNNER_TEMP sibling keep missing"
    unset RUNNER_TEMP
    s1b_owned_workdir "$WORKDIR" || fail "assigned WORKDIR not owned after stripping RUNNER_TEMP"
    sib3=$(mktemp -d /tmp/s1b-XXXXXX)
    s1b_hygiene_track "$sib3"
    printf 'keep\n' >"$sib3/keep"
    if s1b_owned_workdir "$sib3"; then
      fail "tmp prefix sibling owned without RUNNER_TEMP: $sib3"
    fi
    rm -rf "$sib" "$rt" "$sib3"
    remove_owned_workdir "$WORKDIR"
    [[ ! -e "$assigned" ]] || fail "assigned WORKDIR leftover $assigned"
    WORKDIR=""
    echo "ok: cleanup-collision-selftest" ;;
  cleanup-busy-leaf-selftest)
    LEAF=$(mktemp -d)
    s1b_hygiene_track "$LEAF"
    printf 'stuck\n' >"$LEAF/stuck"
    WORKDIR=""
    FIFO=""
    MONITOR_PID=""
    HARNESS_PID=""
    set +e
    cleanup
    crc=$?
    set -e
    [[ "$crc" -ne 0 ]] || fail "nonempty LEAF cleanup returned 0"
    [[ -d "$LEAF" ]] || fail "nonempty LEAF was removed"
    [[ -f "$LEAF/stuck" ]] || fail "nonempty LEAF contents were removed"
    rm -rf "$LEAF"
    LEAF=""
    echo "ok: cleanup-busy-leaf-selftest" ;;
  cleanup-preserve-rc-selftest)
    LEAF=$(mktemp -d)
    s1b_hygiene_track "$LEAF"
    printf 'stuck\n' >"$LEAF/stuck"
    WORKDIR=""
    FIFO=""
    MONITOR_PID=""
    HARNESS_PID=""
    echo "LEAF=$LEAF"
    echo "ok: cleanup-preserve-rc-selftest about to exit 7"
    exit 7 ;;
  cleanup-create-selftest)
    if ! declare -F create_owned_workdir >/dev/null; then
      fail "create_owned_workdir missing"
    fi
    victim=$(mktemp -d /tmp/ruley-keep-XXXXXX)
    s1b_hygiene_track "$victim"
    printf 'keep\n' >"$victim/keep"
    WORKDIR=$victim
    if create_owned_workdir; then
      fail "create_owned_workdir accepted foreign existing $victim"
    fi
    [[ -f "$victim/keep" ]] || fail "foreign WORKDIR was deleted: $victim"
    sib=$(mktemp -d /tmp/s1b-XXXXXX)
    s1b_hygiene_track "$sib"
    printf 'keep\n' >"$sib/keep"
    WORKDIR=$sib
    if create_owned_workdir; then
      fail "create_owned_workdir accepted existing in-namespace $sib"
    fi
    [[ -f "$sib/keep" ]] || fail "existing in-namespace keep missing: $sib"
    historical="${RUNNER_TEMP:-/tmp}/s1b-send-matrix"
    historical_preexisted=0
    if [[ -e "$historical" ]]; then
      historical_preexisted=1
    else
      mkdir "$historical"
      s1b_hygiene_track "$historical"
      printf 'keep\n' >"$historical/keep"
      WORKDIR=$historical
      if create_owned_workdir; then
        fail "create_owned_workdir accepted existing historical $historical"
      fi
      [[ -f "$historical/keep" ]] || fail "historical keep missing: $historical"
    fi
    trav_int="/tmp/s1b-trav-$$"
    trav_tgt="/tmp/s1b-trav-tgt-$$"
    s1b_hygiene_track "$trav_int"
    s1b_hygiene_track "$trav_tgt"
    WORKDIR="$trav_int/../$(basename "$trav_tgt")"
    if create_owned_workdir; then
      fail "create_owned_workdir accepted traversal WORKDIR=$WORKDIR"
    fi
    [[ ! -e "$trav_int" ]] || fail "traversal created intermediate $trav_int"
    [[ ! -e "$trav_tgt" ]] || fail "traversal created target $trav_tgt"
    WORKDIR=""
    create_owned_workdir || fail "first default create refused"
    wd1=$WORKDIR
    if [[ "$wd1" == "$historical" && "$historical_preexisted" -eq 1 ]]; then
      WORKDIR=""
      S1B_OWNED_WORKDIR=""
      fail "default create used pre-existing historical name $historical"
    fi
    s1b_hygiene_track "$wd1"
    [[ "$wd1" != "$historical" ]] || fail "first default create used historical name $historical"
    WORKDIR=""
    create_owned_workdir || fail "second default create refused"
    wd2=$WORKDIR
    if [[ "$wd2" == "$historical" && "$historical_preexisted" -eq 1 ]]; then
      WORKDIR=""
      S1B_OWNED_WORKDIR=""
      fail "second default create used pre-existing historical name $historical"
    fi
    s1b_hygiene_track "$wd2"
    [[ "$wd2" != "$historical" ]] || fail "second default create used historical name $historical"
    [[ "$wd1" != "$wd2" ]] || fail "two default creates produced the same path $wd1"
    if s1b_owned_workdir "$wd1"; then
      fail "first default still owned after second create"
    fi
    s1b_owned_workdir "$wd2" || fail "second default not owned"
    rt="${RUNNER_TEMP:-/tmp}"
    probe="$rt/s1b-positive-probe-$$"
    [[ ! -e "$probe" ]] || fail "probe already exists: $probe"
    WORKDIR=$probe
    create_owned_workdir || fail "caller-provided missing in-namespace refused: $probe"
    s1b_hygiene_track "$probe"
    s1b_owned_workdir "$probe" || fail "caller-provided create not owned: $probe"
    [[ -d "$probe" ]] || fail "caller-provided create did not mkdir: $probe"
    WORKDIR=""
    echo "ok: cleanup-create-selftest" ;;
  cleanup-zero-status-leaf-selftest)
    LEAF=$(mktemp -d)
    s1b_hygiene_track "$LEAF"
    printf 'stuck\n' >"$LEAF/stuck"
    WORKDIR=""
    FIFO=""
    MONITOR_PID=""
    HARNESS_PID=""
    echo "LEAF=$LEAF"
    echo "ok: cleanup-zero-status-leaf-selftest about to exit 0"
    exit 0 ;;
  cleanup-rebind-selftest)
    [[ -x "${HARNESS_BIN:-}" ]] || fail "cleanup-rebind-selftest requires HARNESS_BIN"
    WORKDIR=""
    create_owned_workdir || fail "owned WORKDIR create failed"
    echo "CREATED=$WORKDIR"
    printf 'owned\n' >"$WORKDIR/owned-marker"
    FIFO="$WORKDIR/go.fifo"
    LOG="$WORKDIR/monitor.log"
    HOUT="$WORKDIR/harness.out"
    OH="$WORKDIR/observation-health.json"
    rm -f "$FIFO" "$LOG" "$HOUT" "$OH"
    mkfifo "$FIFO"
    echo "kernel=$(uname -r) host=$(uname -n) mode=$MODE"
    "$HARNESS_BIN" "$FIFO" >"$HOUT" 2>&1
    echo "ok: cleanup-rebind-selftest about to exit 0"
    exit 0 ;;
  cleanup-hygiene-inherit-selftest)
    own=$(mktemp -d /tmp/s1b-hyg-own-XXXXXX)
    s1b_hygiene_track "$own"
    printf 'own\n' >"$own/own"
    echo "OWN=$own"
    echo "ok: cleanup-hygiene-inherit-selftest" ;;
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
  *) fail "usage: $0 positive|attach-disabled|disable-send-attach|cleanup-selftest|cleanup-collision-selftest|cleanup-busy-leaf-selftest|cleanup-preserve-rc-selftest|cleanup-create-selftest|cleanup-zero-status-leaf-selftest|cleanup-hygiene-inherit-selftest|cleanup-rebind-selftest|coverage-gate|mutation-selftest|endpoint-line-selftest|harness-ok-selftest|ringbuf-drop-selftest|send-observation-selftest|monitor-shutdown-selftest|diagnostics-selftest" ;;
esac
