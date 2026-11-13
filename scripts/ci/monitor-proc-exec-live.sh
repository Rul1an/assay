#!/usr/bin/env bash
# Live check of `assay monitor` on a real kernel: a monitored process that execs a program named by a
# `proc_exec` rule with `trigger_kill` is killed, and a process that execs another program is not.
#
# Needs root (eBPF) and the two build outputs: target/release/assay and target/assay-ebpf.o.
# The programs are private copies of `sleep` under names nothing else on the machine uses, so
# `--monitor-all` cannot make the rule match an unrelated process.
#
# Three runs, so the kill is attributed to the rule and to nothing else:
#   1. rule with trigger_kill + enabled kill switch: the named program is killed, the other is not.
#      The same run also shows how an exec is recorded: started by a relative path the program is
#      still matched by a `**/name` pattern and killed; started through a symlink with another name
#      it is recorded under that name, not matched, and runs to its end;
#   2. the same rule with `alert`: the named program runs to its end (the kill came from trigger_kill);
#   3. the same policy without the kill switch: `assay monitor` refuses to start (exit 2).
set -euo pipefail

ASSAY="${ASSAY_BIN:-./target/release/assay}"
OBJ="${ASSAY_EBPF_OBJ:-target/assay-ebpf.o}"
test -x "$ASSAY" && test -f "$OBJ"

WORK="$(mktemp -d)"
trap 'sudo pkill -x assay 2>/dev/null || true; rm -rf "$WORK"' EXIT
cp "$(command -v sleep)" "$WORK/assay-live-forbidden-tool"
cp "$(command -v sleep)" "$WORK/assay-live-allowed-tool"
ln -s assay-live-forbidden-tool "$WORK/assay-live-alias"

policy() { # <file> <action> <kill switch yaml or "">
  cat > "$1" <<YAML
version: "2.0"
name: proc-exec-live
$3
runtime_monitor:
  enabled: true
  rules:
    - id: no-forbidden-tool
      type: proc_exec
      match:
        path_globs: ['**/assay-live-forbidden-tool']
      action: $2
YAML
}

start_monitor() { # <policy> <log>
  # The log is this user's file on purpose; only the monitor needs root.
  # shellcheck disable=SC2024
  sudo -E "$ASSAY" monitor --monitor-all --ebpf "$OBJ" --policy "$1" --duration 45s --print > "$2" 2>&1 &
  MONITOR_PID=$!
  for _ in $(seq 1 60); do
    grep -q "Assay Monitor running" "$2" 2>/dev/null && return 0
    # Not `kill -0`: the background process is sudo, and once it has become root an unprivileged
    # signal is refused, which reads the same as "gone". Run 43375870415 failed here in 4 ms.
    [[ -d "/proc/$MONITOR_PID" ]] || break
    sleep 0.5
  done
  echo "LIVE FAIL: assay monitor did not reach the running state" >&2
  sed 's/^/    /' "$2" >&2 || true
  return 1
}

stop_monitor() {
  sudo pkill -INT -x assay 2>/dev/null || true
  wait "$MONITOR_PID" 2>/dev/null || true
}

run_tool() { # <program> <seconds>: prints "<exit code> <elapsed seconds>"
  local begin code=0
  begin=$(date +%s)
  "$1" "$2" || code=$?
  echo "$code $(( $(date +%s) - begin ))"
}

echo "== kernel: $(uname -r) $(uname -m)"

echo "== 1. trigger_kill with an enabled kill switch"
policy "$WORK/kill.yaml" trigger_kill $'kill_switch:\n  enabled: true\n  mode: immediate'
start_monitor "$WORK/kill.yaml" "$WORK/kill.log"
read -r allowed_code allowed_secs <<<"$(run_tool "$WORK/assay-live-allowed-tool" 2)"
read -r forbidden_code forbidden_secs <<<"$(run_tool "$WORK/assay-live-forbidden-tool" 30)"
read -r relative_code relative_secs <<<"$(cd "$WORK" && run_tool ./assay-live-forbidden-tool 30)"
read -r alias_code alias_secs <<<"$(run_tool "$WORK/assay-live-alias" 3)"
sleep 1
stop_monitor
echo "allowed tool:   exit=$allowed_code after ${allowed_secs}s"
echo "forbidden tool: exit=$forbidden_code after ${forbidden_secs}s"
echo "forbidden tool by relative path: exit=$relative_code after ${relative_secs}s"
echo "forbidden tool through a symlink with another name: exit=$alias_code after ${alias_secs}s"
grep -E "VIOLATION|INIT KILL" "$WORK/kill.log" || true
[[ "$allowed_code" == "0" ]] || { echo "LIVE FAIL: the allowed tool did not run to its end" >&2; exit 1; }
[[ "$forbidden_code" -gt 128 && "$forbidden_secs" -lt 20 ]] \
  || { echo "LIVE FAIL: the forbidden tool was not killed (exit $forbidden_code after ${forbidden_secs}s)" >&2; sed 's/^/    /' "$WORK/kill.log" >&2; exit 1; }
grep -q "VIOLATION: Rule 'no-forbidden-tool'" "$WORK/kill.log" \
  || { echo "LIVE FAIL: the monitor did not report the rule" >&2; sed 's/^/    /' "$WORK/kill.log" >&2; exit 1; }
# A relative exec is recorded without its directory; the `**/name` pattern still names it.
[[ "$relative_code" -gt 128 && "$relative_secs" -lt 20 ]] \
  || { echo "LIVE FAIL: the tool started by a relative path was not killed (exit $relative_code after ${relative_secs}s)" >&2; sed 's/^/    /' "$WORK/kill.log" >&2; exit 1; }
# The limit the guide states: an exec is matched on the path the kernel recorded, so a symlink with
# another name is another path. If this ever starts matching, the guide is what has to change.
[[ "$alias_code" == "0" ]] \
  || { echo "LIVE FAIL: the tool started through a symlink was killed (exit $alias_code); the guide says it is recorded under the symlink's name" >&2; exit 1; }
[[ "$(grep -c "VIOLATION: Rule 'no-forbidden-tool'" "$WORK/kill.log")" == "2" ]] \
  || { echo "LIVE FAIL: expected two violations (full path and relative path), got $(grep -c "VIOLATION" "$WORK/kill.log")" >&2; sed 's/^/    /' "$WORK/kill.log" >&2; exit 1; }

echo "== 2. the same rule with alert: reported, not killed"
policy "$WORK/alert.yaml" alert ""
start_monitor "$WORK/alert.yaml" "$WORK/alert.log"
read -r alert_code alert_secs <<<"$(run_tool "$WORK/assay-live-forbidden-tool" 3)"
sleep 1
stop_monitor
echo "forbidden tool under alert: exit=$alert_code after ${alert_secs}s"
[[ "$alert_code" == "0" ]] || { echo "LIVE FAIL: an alert rule killed the tool" >&2; exit 1; }
grep -q "VIOLATION: Rule 'no-forbidden-tool'" "$WORK/alert.log" \
  || { echo "LIVE FAIL: the alert rule was not reported" >&2; sed 's/^/    /' "$WORK/alert.log" >&2; exit 1; }

echo "== 3. trigger_kill without a kill switch: refused at start"
policy "$WORK/noswitch.yaml" trigger_kill ""
refused=0
# shellcheck disable=SC2024
sudo -E "$ASSAY" monitor --monitor-all --ebpf "$OBJ" --policy "$WORK/noswitch.yaml" --duration 5s > "$WORK/noswitch.log" 2>&1 || refused=$?
echo "exit=$refused"
grep -E "rule 'no-forbidden-tool'|FATAL" "$WORK/noswitch.log" || true
[[ "$refused" == "2" ]] || { echo "LIVE FAIL: expected exit 2, got $refused" >&2; sed 's/^/    /' "$WORK/noswitch.log" >&2; exit 1; }
grep -q "rule 'no-forbidden-tool': trigger_kill on proc_exec needs an enabled kill_switch" "$WORK/noswitch.log" \
  || { echo "LIVE FAIL: the refusal does not name the rule" >&2; exit 1; }

echo "LIVE PASS: an exec named by a proc_exec trigger_kill rule was killed; the controls behaved as stated."
