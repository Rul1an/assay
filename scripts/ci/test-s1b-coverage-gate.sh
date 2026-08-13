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

pos='  • Send observation:   sendto emitted=1 dropped=0 no_peer=1 non_ip=1; sendmsg emitted=1 dropped=0 no_peer=1 non_ip=1'
dis='  • Send observation:   sendto emitted=0 dropped=0 no_peer=0 non_ip=0; sendmsg emitted=0 dropped=0 no_peer=0 non_ip=0'
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

set +e
timeout -k 2 12 bash "$DRIVER" cleanup-selftest >"$tmp/cleanup.out" 2>"$tmp/cleanup.err"
cec=$?
set -e
[[ "$cec" -eq 0 ]] || fail "cleanup-selftest ec=$cec err=$(cat "$tmp/cleanup.err")"
grep -q 'ok: cleanup-selftest' "$tmp/cleanup.out" || fail "cleanup-selftest missing ok"
echo "ok: mutation-selftest and cleanup-selftest"

wf="$(cd "$(dirname "$0")/../.." && pwd)/.github/workflows/monitor-attach-smoke.yml"
n=$(grep -cF 'bash scripts/ci/test-s1b-coverage-gate.sh' "$wf" || true)
[[ "$n" -eq 1 ]] || fail "workflow must invoke the coverage-gate suite exactly once, got $n"
if grep -q 'bash scripts/ci/run-send-syscall-matrix.sh cleanup-selftest' "$wf"; then
  fail "workflow still invokes cleanup-selftest directly"
fi
if grep -q 'bash scripts/ci/run-send-syscall-matrix.sh mutation-selftest' "$wf"; then
  fail "workflow still invokes mutation-selftest directly"
fi
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
