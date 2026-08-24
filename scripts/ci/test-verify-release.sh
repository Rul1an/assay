#!/usr/bin/env bash
# Offline contract tests for verify-release.sh. The live v5.3.0 replay is opt-in:
# bash scripts/ci/verify-release.sh --self-test
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ORACLE="${ROOT}/scripts/ci/verify-release.sh"

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  exit 1
}

expect_status() {
  local expected="$1"
  shift
  local status=0
  "$@" >/dev/null 2>&1 || status=$?
  [[ "$status" -eq "$expected" ]] || fail "expected exit $expected, got $status: $*"
}

[[ -f "$ORACLE" ]] || fail "release oracle is missing: $ORACLE"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

expected_assets="$("$ORACLE" --unit-expected-assets 5.3.0)"
[[ "$(printf '%s\n' "$expected_assets" | wc -l | tr -d ' ')" -eq 23 ]] \
  || fail "asset generator did not produce 23 names"
printf '%s\n' "$expected_assets" | grep -qxF 'assay-v5.3.0-release-proof-kit.tar.gz' \
  || fail "asset generator omitted proof kit"
if printf '%s\n' "$expected_assets" | grep -qxF 'latest.json'; then
  fail "asset generator must not accept latest.json"
fi

# Abbreviated identifiers are never resolved or expanded.
expect_status 1 "$ORACLE" --unit-validate-sha bbb5e7fe4
expect_status 1 env PIN_SHA= "$ORACLE" --pre-tag
expect_status 1 env PIN_SHA=bbb5e7fe4 "$ORACLE" --pre-tag

# The proof/provenance pair and their checksums were intentionally unattested in
# v5.3.0. A 404 for only these four names is expected-absent, not a finding.
for name in \
  assay-v5.3.0-release-proof-kit.tar.gz \
  assay-v5.3.0-release-proof-kit.tar.gz.sha256 \
  assay-v5.3.0-release-provenance.json \
  assay-v5.3.0-release-provenance.json.sha256; do
  [[ "$("$ORACLE" --unit-classify-attestation "$name" 404)" == "expected-absent" ]] \
    || fail "expected-absent attestation was not classified green: $name"
done
expect_status 1 "$ORACLE" --unit-classify-attestation assay-v5.3.0-x86_64-unknown-linux-gnu.tar.gz 404

# GitHub wraps the in-toto statement in a Sigstore bundle. The required
# application/vnd.in-toto+json value belongs to dsseEnvelope.payloadType.
attestation="$tmp/attestation.json"
mkdir -p "$tmp"
printf '%s\n' \
  '{"attestations":[{"bundle":{"mediaType":"application/vnd.dev.sigstore.bundle.v0.3+json","dsseEnvelope":{"payloadType":"application/vnd.in-toto+json"}}}]}' \
  >"$attestation"
expect_status 0 "$ORACLE" --unit-verify-attestation-json "$attestation"

# Mutation proof: treating expected 404s as failures must make the targeted
# assertion red. This checks behavior through a copied oracle, not a mock.
mutant="$tmp/verify-release.sh"
cp "$ORACLE" "$mutant"
python3 - "$mutant" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
needle = 'printf \'%s\\n\' "expected-absent"\n    return 0'
if needle not in text:
    raise SystemExit("expected-absent implementation anchor moved")
path.write_text(text.replace(needle, 'printf \'%s\\n\' "expected-absent"\n    return 1', 1), encoding="utf-8")
PY
expect_status 1 "$mutant" --unit-classify-attestation assay-v5.3.0-release-proof-kit.tar.gz 404

# A heredoc supplies Python's stdin itself, so piping JSON into one silently
# loses the JSON. Release and provenance JSON must be parsed from a path.
if grep -Eq '\|[[:space:]]*(python3|"\$PYTHON")[[:space:]].*<<' "$ORACLE"; then
  fail "oracle pipes JSON into a Python heredoc"
fi
grep -q 'bounded_capture' "$ORACLE" || fail "oracle has no bounded JSON capture"

# A stalled GitHub CLI must be bounded by the verifier itself. The outer Python
# watchdog protects this test from hanging when that inner deadline regresses.
fake_gh="$tmp/gh"
cat >"$fake_gh" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$$" >"$FAKE_GH_PID_FILE"
sleep 30 &
printf '%s\n' "$!" >>"$FAKE_GH_PID_FILE"
wait
SH
chmod +x "$fake_gh"

status_file="$tmp/hung-gh.status"
stderr_file="$tmp/hung-gh.stderr"
pid_file="$tmp/hung-gh.pid"
FAKE_GH_PID_FILE="$pid_file" \
ASSAY_RELEASE_GH_TIMEOUT_SECONDS=0.2 \
GH="$fake_gh" \
python3 - "$ORACLE" "$status_file" "$stderr_file" "$pid_file" <<'PY'
import os
import pathlib
import signal
import subprocess
import sys

oracle, status_path, stderr_path, pid_path = sys.argv[1:]
process = subprocess.Popen(
    [oracle, "--self-test"],
    stdout=subprocess.PIPE,
    stderr=subprocess.PIPE,
    start_new_session=True,
)
try:
    _, stderr = process.communicate(timeout=2.0)
    status = process.returncode
except subprocess.TimeoutExpired:
    os.killpg(process.pid, signal.SIGKILL)
    try:
        pids = pathlib.Path(pid_path).read_text(encoding="utf-8").splitlines()
        if pids:
            os.killpg(int(pids[0]), signal.SIGKILL)
    except (FileNotFoundError, ProcessLookupError, ValueError):
        pass
    _, stderr = process.communicate()
    status = 124
pathlib.Path(status_path).write_text(f"{status}\n", encoding="utf-8")
pathlib.Path(stderr_path).write_bytes(stderr)
PY
[[ "$(cat "$status_file")" -eq 2 ]] \
  || fail "hung gh was not classified as infrastructure exit 2 (got $(cat "$status_file"))"
grep -q 'deadline' "$stderr_file" || fail "hung gh diagnostic does not name the deadline"
[[ -s "$pid_file" ]] || fail "fake gh did not record its pid"
while IFS= read -r pid; do
  if kill -0 "$pid" 2>/dev/null; then
    fail "hung gh process $pid survived verifier timeout"
  fi
done <"$pid_file"
[[ "$(wc -l <"$pid_file" | tr -d ' ')" -eq 2 ]] \
  || fail "fake gh did not record both parent and descendant pids"

for invalid_timeout in 0 nan invalid; do
  status=0
  diagnostic="$(
    ASSAY_RELEASE_GH_TIMEOUT_SECONDS="$invalid_timeout" GH="$fake_gh" \
      "$ORACLE" --self-test 2>&1 >/dev/null
  )" || status=$?
  [[ "$status" -eq 2 ]] \
    || fail "invalid gh deadline '$invalid_timeout' did not fail as infrastructure"
  printf '%s\n' "$diagnostic" | grep -q 'must be a positive finite number' \
    || fail "invalid gh deadline '$invalid_timeout' lacks a bounded diagnostic"
done

printf 'PASS: verify-release offline contract tests\n'
