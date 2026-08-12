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
bash "$DRIVER" cleanup-selftest
echo "ok: mutation-selftest and cleanup-selftest"

wf="$(cd "$(dirname "$0")/../.." && pwd)/.github/workflows/monitor-attach-smoke.yml"
grep -q 'bash scripts/ci/run-send-syscall-matrix.sh cleanup-selftest' "$wf" \
  || fail "workflow does not invoke cleanup-selftest"
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
