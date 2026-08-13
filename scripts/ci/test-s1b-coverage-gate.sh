#!/usr/bin/env bash
# Contract: coverage gate allows only CLI-reachable labels; mutation/cleanup selftests.
set -euo pipefail
DRIVER="$(cd "$(dirname "$0")" && pwd)/run-send-syscall-matrix.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

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

bash "$DRIVER" mutation-selftest

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
grep -Fq 'fn format_send_observation_summary' "$cli_out" \
  || fail "format_send_observation_summary missing from output.rs"
grep -Fq "$so_fmt" "$cli_out" \
  || fail "send observation formatter missing from output.rs format_send_observation_summary"
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
grep -Fq 'monitor_shutdown_ok "$mc"' "$DRIVER" \
  || fail "run_matrix does not assert controlled monitor shutdown"
echo "ok: monitor SIGINT shutdown is exit 0, not 130/crash"

wait_iters=$(sed -n 's/^WAIT_LOG_ITERS=//p' "$DRIVER" | head -1)
wait_sleep=$(sed -n 's/^WAIT_LOG_SLEEP_S=//p' "$DRIVER" | head -1)
wait_max=$(sed -n 's/^WAIT_LOG_MAX_BEFORE_GO=//p' "$DRIVER" | head -1)
go_margin=$(sed -n 's/^GO_TIMEOUT_MARGIN_MS=//p' "$DRIVER" | head -1)
go_ms=$(sed -n 's/^#define GO_FIFO_TIMEOUT_MS //p' "$(cd "$(dirname "$0")" && pwd)/s1b-send-syscall-matrix.c" | head -1)
[[ -n "$wait_iters" && -n "$wait_sleep" && -n "$wait_max" && -n "$go_margin" && -n "$go_ms" ]] \
  || fail "missing GO/wait_log bound constants"
need=$(python3 -c "print(int($wait_max * $wait_iters * float('$wait_sleep') * 1000 + $go_margin))")
[[ "$go_ms" -ge "$need" ]] || fail "GO_FIFO_TIMEOUT_MS=$go_ms < $wait_max*$wait_iters*${wait_sleep}s*1000+$go_margin=$need"
grep -Fq 'n < WAIT_LOG_ITERS' "$DRIVER" || fail "wait_log does not use WAIT_LOG_ITERS"
# shellcheck disable=SC2016
grep -Fq 'sleep "$WAIT_LOG_SLEEP_S"' "$DRIVER" || fail "wait_log does not use WAIT_LOG_SLEEP_S"
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
timeout -k 2 12 bash "$DRIVER" cleanup-selftest >"$tmp/cleanup.out" 2>"$tmp/cleanup.err"
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
active_run_lines() {
  awk '
    function emit_active(line, tmp) {
      tmp = line
      sub(/^[[:space:]]+/, "", tmp)
      if (tmp == "" || tmp ~ /^#/) return
      print tmp
    }
    /^        run: \|/ { in_run=1; next }
    in_run && /^        [^[:space:]]/ { exit }
    in_run { emit_active($0) }
  ' <<<"$1"
}
compile_step=$(named_step "$wf" "Compile syscall harness")
[[ -n "$compile_step" ]] || fail "missing Compile syscall harness step"
if grep -Eq '^        continue-on-error[[:space:]]*:' <<<"$compile_step"; then
  fail "Compile syscall harness must not continue-on-error"
fi
if grep -Eq '^        if:[[:space:]]*false([[:space:]]|$)' <<<"$compile_step"; then
  fail "Compile syscall harness must not be disabled with if: false"
fi
grep -Fq "        if: \${{ github.event.inputs.proof_mode == 'send-matrix' }}" <<<"$compile_step" \
  || fail "Compile syscall harness must be send-matrix-conditioned"
compile_active=$(active_run_lines "$compile_step")
grep -qx 'bash scripts/ci/test-s1b-coverage-gate.sh' <<<"$compile_active" \
  || fail "Compile syscall harness must actively run the coverage-gate suite"
pc="$(cd "$(dirname "$0")/../.." && pwd)/.pre-commit-config.yaml"
grep -Fq 'id: s1b-coverage-gate-contract' "$pc" \
  || fail "pre-commit must run the coverage-gate suite as an external contract"
grep -Fq 'entry: bash scripts/ci/test-s1b-coverage-gate.sh' "$pc" \
  || fail "pre-commit hook must execute the coverage-gate suite"
if grep -q 'bash scripts/ci/run-send-syscall-matrix.sh cleanup-selftest' "$wf"; then
  fail "workflow still invokes cleanup-selftest directly"
fi
if grep -q 'bash scripts/ci/run-send-syscall-matrix.sh mutation-selftest' "$wf"; then
  fail "workflow still invokes mutation-selftest directly"
fi
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
[[ -n "$compile_idx" && -n "$positive_idx" && "$compile_idx" -lt "$positive_idx" ]] \
  || fail "coverage-gate suite step must run before the positive matrix"
preclean_run=$(awk '
  /^      - name: Pre-clean self-hosted workspace$/ { in_step=1; next }
  in_step && /^      - name: / { exit }
  in_step && /^        run: \|$/ { in_run=1; next }
  in_run {
    if ($0 ~ /^          / || $0 == "") { print; next }
    exit
  }
' "$wf")
[[ -n "$preclean_run" ]] || fail "could not extract first Pre-clean run: block"
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
grep -Fq 'sudo pkill -x assay' <<<"$preclean_run" || fail "Pre-clean run: must pkill assay"
# shellcheck disable=SC2016
grep -Fq 'sudo chattr -R -i' <<<"$preclean_run" || fail "Pre-clean run: must chattr -i leftover immutable files"
# shellcheck disable=SC2016
grep -Fq 'find "$GITHUB_WORKSPACE" -mindepth 1 -maxdepth 1 -exec sudo rm -rf {} +' <<<"$preclean_run" \
  || fail "Pre-clean run: must sudo-delete all top-level workspace entries"
# shellcheck disable=SC2016
grep -Fq 'find "$GITHUB_WORKSPACE" -mindepth 1 -maxdepth 1 -print -quit' <<<"$preclean_run" \
  || fail "Pre-clean run: must fail-closed assert the workspace is empty"
grep -Fq 'ERROR: workspace not empty after' <<<"$preclean_run" \
  || fail "Pre-clean run: must fail-closed if the workspace is not empty"
nonempty_branch=$(awk '
  /mindepth 1 -maxdepth 1 -print -quit/ { p=1 }
  p { print }
  p && /[[:space:]]fi[[:space:]]*$/ { exit }
' <<<"$preclean_run")
grep -Eq '^[[:space:]]+exit 1[[:space:]]*$' <<<"$nonempty_branch" \
  || fail "Pre-clean nonempty branch must fail-closed with exit 1"
preclean_step=$(named_step "$wf" "Pre-clean self-hosted workspace")
if grep -Eq '^        continue-on-error[[:space:]]*:' <<<"$preclean_step"; then
  fail "Pre-clean must not continue-on-error"
fi
if grep -Eq '^        if:' <<<"$preclean_step"; then
  fail "Pre-clean must not have a disabling if"
fi
echo "ok: pre-clean run: block scoped before checkout"
# Intentional: match the workflow's literal backup copy/mv/cmp, not expand here.
# shellcheck disable=SC2016
grep -Fq 'cp "$binbak"' "$wf" || fail "workflow restore does not copy pre-mutation backup"
# shellcheck disable=SC2016
grep -Fq 'mv -f "$bin.tmp" "$bin"' "$wf" || fail "workflow restore does not atomically mv backup onto assay"
# shellcheck disable=SC2016
grep -Fq 'cmp -s "$bin" "$binbak"' "$wf" || fail "workflow restore does not verify restored binary matches backup"
restore_fn=$(awk '/restore\(\) \{/,/^          \}/' "$wf")
[[ -n "$restore_fn" ]] || fail "could not extract restore() from workflow"
if grep -q 'cargo build' <<<"$restore_fn"; then
  fail "workflow restore must not cargo build"
fi
grep -q 'elapsed_ms' "$(cd "$(dirname "$0")" && pwd)/s1b-send-syscall-matrix.c" \
  || fail "timeout selftest does not assert elapsed_ms"

sim=$(mktemp -d)
printf 'canonical-src\n' >"$sim/src"
printf 'canonical-src\n' >"$sim/bak"
printf 'canonical-bin\n' >"$sim/binbak"
printf 'MUTATED-s1b-cell7-disabled\n' >"$sim/bin"
cp "$sim/bak" "$sim/src"
cp "$sim/binbak" "$sim/bin.tmp"
cmp -s "$sim/bin.tmp" "$sim/binbak" || fail "temp copy hash mismatch"
mv "$sim/bin.tmp" "$sim/bin"
cmp -s "$sim/bin" "$sim/binbak" || fail "restored binary does not match pre-mutation backup"
cmp -s "$sim/src" "$sim/bak" || fail "restore simulation did not restore source"
rm -rf "$sim"
echo "ok: restore copies exact pre-mutation binary; no cargo build in restore"
