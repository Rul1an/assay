#!/usr/bin/env bash
# Contract: coverage gate allows only CLI-reachable labels; mutation/cleanup selftests.
set -euo pipefail
if [[ "${S1B_COVERAGE_GATE_FILESELECT_PROBE:-}" == 1 ]]; then
  exit 0
fi
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
run_matrix_active=$(awk '
  /^run_matrix\(\)/ { p=1 }
  p {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp != "" && tmp !~ /^#/) print tmp
  }
  p && /^}/ { exit }
' "$DRIVER")
grep -Fq 'monitor_shutdown_ok "$mc"' <<<"$run_matrix_active" \
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
wait_log_active=$(awk '
  /^wait_log\(\)/ { p=1 }
  p {
    tmp = $0
    sub(/^[[:space:]]+/, "", tmp)
    if (tmp != "" && tmp !~ /^#/) print tmp
  }
  p && /^}/ { exit }
' "$DRIVER")
grep -Fq 'n < WAIT_LOG_ITERS' <<<"$wait_log_active" || fail "wait_log does not use WAIT_LOG_ITERS"
# shellcheck disable=SC2016
grep -Fq 'sleep "$WAIT_LOG_SLEEP_S"' <<<"$wait_log_active" || fail "wait_log does not use WAIT_LOG_SLEEP_S"
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

timeout_bin=""
if command -v timeout >/dev/null 2>&1; then
  timeout_bin=timeout
elif command -v gtimeout >/dev/null 2>&1; then
  timeout_bin=gtimeout
else
  echo "note: timeout(1) absent; Linux send-matrix proof owns the hang bound (macOS compile is not a claim)" >&2
fi
set +e
if [[ -n "$timeout_bin" ]]; then
  "$timeout_bin" -k 2 12 bash "$DRIVER" cleanup-selftest >"$tmp/cleanup.out" 2>"$tmp/cleanup.err"
else
  bash "$DRIVER" cleanup-selftest >"$tmp/cleanup.out" 2>"$tmp/cleanup.err"
fi
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
  local name="$1" want="$2" step body
  step=$(named_step "$wf" "$name")
  [[ -n "$step" ]] || fail "missing step: $name"
  if grep -Eq '^        continue-on-error[[:space:]]*:' <<<"$step"; then
    fail "$name must not continue-on-error"
  fi
  grep -qx "        if: \${{ github.event.inputs.proof_mode == 'send-matrix' }}" <<<"$step" \
    || fail "$name must be send-matrix-conditioned"
  body=$(active_lines "$(normalized_run_body "$step")")
  [[ "$body" == "$want" ]] || fail "$name run body must be exactly: $want (got: ${body:-<empty>})"
  if grep -Eq '\\$|\|\| true|<<|function |if false' <<<"$body"; then
    fail "$name run body is not a single command"
  fi
}
compile_step=$(named_step "$wf" "Compile syscall harness")
[[ -n "$compile_step" ]] || fail "missing Compile syscall harness step"
assert_single_cmd_step "Compile syscall harness" \
  'bash scripts/ci/s1b-compile-and-contract.sh'
assert_single_cmd_step "Positive syscall/effect matrix" \
  'bash scripts/ci/s1b-positive-matrix.sh'
assert_single_cmd_step "Attach-disabled negative matrix" \
  'bash scripts/ci/s1b-attach-disabled.sh'
POS="$(cd "$(dirname "$0")" && pwd)/s1b-positive-matrix.sh"
if grep -q '<<' "$POS"; then
  fail "positive wrapper must not hide the driver in a heredoc"
fi
pos_active=$(active_lines "$(cat "$POS")")
# shellcheck disable=SC2016
grep -Fq 'sudo -E env HARNESS_BIN="${RUNNER_TEMP:?}/s1b-harness" WORKDIR="${RUNNER_TEMP:?}/s1b-positive" \' \
  <<<"$pos_active" || fail "positive wrapper must live-invoke sudo -E env for the matrix"
grep -Fq 'bash scripts/ci/run-send-syscall-matrix.sh positive' <<<"$pos_active" \
  || fail "positive wrapper must invoke run-send-syscall-matrix.sh positive"
if grep -Eq '^echo would-have-run' <<<"$pos_active"; then
  fail "positive wrapper replaced the live sudo with a dry-run echo"
fi
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
  ".pre-commit-config.yaml" \
  || fail "s1b pre-commit hook YAML contract failed without ruby on PATH"
echo "ok: s1b hook YAML contract without ruby"
hook_wf_out=$(S1B_COVERAGE_GATE_FILESELECT_PROBE=1 pre-commit run s1b-coverage-gate-contract \
  --files .github/workflows/monitor-attach-smoke.yml 2>&1) || true
printf '%s\n' "$hook_wf_out" | grep -q 'Passed' \
  || fail "real pre-commit must run the hook on the smoke workflow"
if printf '%s\n' "$hook_wf_out" | grep -q 'Skipped'; then
  fail "real pre-commit skipped the smoke workflow"
fi
hook_skip_out=$(S1B_COVERAGE_GATE_FILESELECT_PROBE=1 pre-commit run s1b-coverage-gate-contract \
  --files crates/assay-cli/src/lib.rs 2>&1) || true
printf '%s\n' "$hook_skip_out" | grep -q 'Skipped' \
  || fail "real pre-commit must skip unrelated lib.rs"
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
if [[ -n "$timeout_bin" ]]; then
  "$timeout_bin" -k 2 8 env ASSAY_BIN="$a10/assay" HARNESS_BIN="$a10/harness" ASSAY_EBPF="$a10/ebpf.o" \
    bash "$DRIVER" attach-disabled >"$tmp/a10.out" 2>"$tmp/a10.err"
else
  env ASSAY_BIN="$a10/assay" HARNESS_BIN="$a10/harness" ASSAY_EBPF="$a10/ebpf.o" \
    bash "$DRIVER" attach-disabled >"$tmp/a10.out" 2>"$tmp/a10.err"
fi
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
prod_active=$(active_lines "$(awk '
  /^if \[\[ "\$\{1:-\}" == restore-selftest/ { skip=1 }
  skip && /^fi$/ { skip=0; next }
  skip { next }
  { print }
' "$ATTACH")")
if grep -Eq '^bin=' <<<"$prod_active"; then
  fail "production attach-disabled must not rebind bin before restore"
fi
bash "$ATTACH" restore-selftest
grep -q 'elapsed_ms' "$(cd "$(dirname "$0")" && pwd)/s1b-send-syscall-matrix.c" \
  || fail "timeout selftest does not assert elapsed_ms"
echo "ok: restore copies exact pre-mutation binary; no cargo build in restore"
