#!/usr/bin/env bash
# Offline contract tests for verify-release.sh. The live v5.3.0 replay is opt-in:
# bash scripts/ci/verify-release.sh --self-test
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ORACLE="${ROOT}/scripts/ci/verify-release.sh"
ASSET_CONTRACT="${ROOT}/scripts/ci/release_asset_contract.sh"
RELEASE_TAG_READER="${ROOT}/scripts/ci/read-assay-release-tag.sh"

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
[[ -f "$ASSET_CONTRACT" ]] || fail "shared release asset contract is missing: $ASSET_CONTRACT"
[[ -f "$RELEASE_TAG_READER" ]] || fail "release tag reader is missing: $RELEASE_TAG_READER"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

release_tag="$(bash "$RELEASE_TAG_READER")"

candidate_paths=(
  Cargo.toml .github/assay-release-tag README.md docs/index.md
  docs/getting-started/index.md docs/getting-started/installation.md
  docs/getting-started/quickstart.md docs/getting-started/ci-integration.md
  docs/reference/cli/index.md docs/reference/release.md docs/AIcontext/user-flows.md docs/use-cases/ci-gate.md
)

make_candidate() {
  local destination="$1" version="$2"
  local path
  for path in "${candidate_paths[@]}"; do
    mkdir -p "$destination/$(dirname "$path")"
    cp "$ROOT/$path" "$destination/$path"
  done
  python3 - "$destination/Cargo.toml" "$version" <<'PY'
import pathlib
import re
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
section = re.search(r"(?ms)^\[workspace\.package\]$(.*?)(?=^\[|\Z)", text)
if not section:
    raise SystemExit("workspace.package section is missing")
replacement = re.sub(
    r'^version\s*=\s*"[^"]+"\s*$',
    f'version = "{sys.argv[2]}"',
    section.group(1),
    count=1,
    flags=re.M,
)
if not re.search(r'^version\s*=\s*"[^"]+"\s*$', section.group(1), re.M):
    raise SystemExit("workspace.package version is missing")
path.write_text(text[: section.start(1)] + replacement + text[section.end(1) :], encoding="utf-8")
PY
}

fixture_gh="$tmp/fixture-gh"
cat >"$fixture_gh" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == api && "${2:-}" =~ ^repos/Rul1an/assay/contents/(.*)\?ref=([0-9a-f]{40})$ ]] \
  || exit 64
path="${BASH_REMATCH[1]}"
[[ "${BASH_REMATCH[2]}" == "${PIN_SHA:-}" ]] || exit 65
python3 - "$FIXTURE_ROOT/$path" <<'PY'
import base64
import json
import pathlib
import sys

print(json.dumps({"content": base64.b64encode(pathlib.Path(sys.argv[1]).read_bytes()).decode()}))
PY
SH
chmod +x "$fixture_gh"

# Candidate source version and last-published install truth are independent.
# A release-prep tree may lead the install pin until the new release exists.
current_candidate="$tmp/current-candidate"
make_candidate "$current_candidate" 5.4.0
expect_status 0 env FIXTURE_ROOT="$current_candidate" GH="$fixture_gh" \
  VERSION=5.4.0 PIN_SHA="$(git -C "$ROOT" rev-parse HEAD)" "$ORACLE" --pre-tag

next_candidate="$tmp/next-candidate"
make_candidate "$next_candidate" 5.5.0
expect_status 0 env FIXTURE_ROOT="$next_candidate" GH="$fixture_gh" \
  VERSION=5.5.0 PIN_SHA="$(git -C "$ROOT" rev-parse HEAD)" "$ORACLE" --pre-tag

drift_candidate="$tmp/drift-candidate"
cp -R "$current_candidate" "$drift_candidate"
printf 'v5.3.0\n' >"$drift_candidate/.github/assay-release-tag"
expect_status 1 env FIXTURE_ROOT="$drift_candidate" GH="$fixture_gh" \
  VERSION=5.4.0 PIN_SHA="$(git -C "$ROOT" rev-parse HEAD)" "$ORACLE" --pre-tag

malformed_candidate="$tmp/malformed-candidate"
cp -R "$current_candidate" "$malformed_candidate"
printf 'latest\n' >"$malformed_candidate/.github/assay-release-tag"
expect_status 1 env FIXTURE_ROOT="$malformed_candidate" GH="$fixture_gh" \
  VERSION=5.4.0 PIN_SHA="$(git -C "$ROOT" rev-parse HEAD)" "$ORACLE" --pre-tag

# Mutation proof: restoring the old release-specific literal must fail on the
# current candidate instead of turning the next release into another edit site.
pin_mutant_dir="$tmp/pin-mutant"
mkdir -p "$pin_mutant_dir"
cp "$ORACLE" "$ASSET_CONTRACT" "$RELEASE_TAG_READER" "$pin_mutant_dir/"
python3 - "$pin_mutant_dir/verify-release.sh" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
dynamic_pin = 'pin = f"cargo install assay-cli --version {published_version} --locked"'
dynamic_link = 'link = f"Current release: [`{published_tag}`](https://github.com/Rul1an/assay/releases/tag/{published_tag})"'
if text.count(dynamic_pin) != 1 or text.count(dynamic_link) != 1:
    raise SystemExit("published release derivation anchors moved")
text = text.replace(dynamic_pin, 'pin = "cargo install assay-cli --version 5.3.0 --locked"', 1)
text = text.replace(
    dynamic_link,
    'link = "Current release: [`v5.3.0`](https://github.com/Rul1an/assay/releases/tag/v5.3.0)"',
    1,
)
path.write_text(text, encoding="utf-8")
PY
expect_status 1 env FIXTURE_ROOT="$current_candidate" GH="$fixture_gh" \
  VERSION=5.4.0 PIN_SHA="$(git -C "$ROOT" rev-parse HEAD)" \
  "$pin_mutant_dir/verify-release.sh" --pre-tag

# Mutation proof: the source fetch must stay bound to the requested candidate
# commit, not merely to the expected repository path.
ref_mutant_dir="$tmp/ref-mutant"
mkdir -p "$ref_mutant_dir"
cp "$ORACLE" "$ASSET_CONTRACT" "$RELEASE_TAG_READER" "$ref_mutant_dir/"
python3 - "$ref_mutant_dir/verify-release.sh" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
anchor = 'repos/$REPO/contents/$path?ref=$sha'
if text.count(anchor) != 1:
    raise SystemExit("candidate source ref anchor moved")
text = text.replace(anchor, f"repos/$REPO/contents/$path?ref={'0' * 40}", 1)
path.write_text(text, encoding="utf-8")
PY
expect_status 2 env FIXTURE_ROOT="$current_candidate" GH="$fixture_gh" \
  VERSION=5.4.0 PIN_SHA="$(git -C "$ROOT" rev-parse HEAD)" \
  "$ref_mutant_dir/verify-release.sh" --pre-tag

expected_assets="$("$ORACLE" --unit-expected-assets 5.3.0)"
[[ "$(printf '%s\n' "$expected_assets" | wc -l | tr -d ' ')" -eq 23 ]] \
  || fail "asset generator did not produce 23 names"
printf '%s\n' "$expected_assets" | grep -qxF 'assay-v5.3.0-release-proof-kit.tar.gz' \
  || fail "asset generator omitted proof kit"
if printf '%s\n' "$expected_assets" | grep -qxF 'latest.json'; then
  fail "asset generator must not accept latest.json"
fi

expected_mcp_installability=$(cat <<EOF
assay-mcp-server	x86_64-unknown-linux-gnu	manual_step	assay-mcp-server-${release_tag}-x86_64-unknown-linux-gnu.tar.gz
assay-mcp-server	aarch64-unknown-linux-gnu	manual_step	assay-mcp-server-${release_tag}-aarch64-unknown-linux-gnu.tar.gz
assay-mcp-server	x86_64-apple-darwin	unsupported	-
assay-mcp-server	aarch64-apple-darwin	unsupported	-
assay-mcp-server	x86_64-pc-windows-msvc	unsupported	-
EOF
)
actual_mcp_installability="$($ORACLE --unit-installability-matrix "$release_tag" | grep '^assay-mcp-server')"
[[ "$actual_mcp_installability" == "$expected_mcp_installability" ]] \
  || fail "MCP server installability matrix is not the measured release surface"

release_doc="$ROOT/docs/reference/release.md"
documented_matrix="$(sed -n \
  '/<!-- release-installability-matrix:start -->/,/<!-- release-installability-matrix:end -->/p' \
  "$release_doc" | sed '1d;$d')"
generated_matrix="$($ORACLE --unit-installability-markdown "$release_tag")"
[[ -n "$documented_matrix" && "$documented_matrix" == "$generated_matrix" ]] \
  || fail "release documentation did not come from the installability contract"

# Both release consumers must call the shared contract. Reintroducing a local
# expected_assets function would create a second release truth.
if grep -Eq '^expected_assets\(\)' "$ORACLE"; then
  fail "release verifier carries a local expected_assets implementation"
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
python3 - "$FAKE_GH_PID_FILE" <<'PY' &
import os
import signal
import sys
import time

signal.signal(signal.SIGTERM, signal.SIG_IGN)
with open(sys.argv[1], "a", encoding="utf-8") as handle:
    handle.write(f"{os.getpid()}\n")
os.close(1)
os.close(2)
while True:
    time.sleep(30)
PY
while [[ "$(wc -l <"$FAKE_GH_PID_FILE" | tr -d ' ')" -lt 2 ]]; do
  sleep 0.01
done
wait
SH
chmod +x "$fake_gh"

status_file="$tmp/hung-gh.status"
stderr_file="$tmp/hung-gh.stderr"
pid_file="$tmp/hung-gh.pid"
survivor_file="$tmp/hung-gh.survivors"
FAKE_GH_PID_FILE="$pid_file" \
ASSAY_RELEASE_GH_TIMEOUT_SECONDS=0.2 \
GH="$fake_gh" \
python3 - "$ORACLE" "$status_file" "$stderr_file" "$pid_file" "$survivor_file" <<'PY'
import os
import pathlib
import signal
import subprocess
import sys

oracle, status_path, stderr_path, pid_path, survivor_path = sys.argv[1:]
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
pids = []
try:
    pids = [int(pid) for pid in pathlib.Path(pid_path).read_text(encoding="utf-8").splitlines()]
except (FileNotFoundError, ValueError):
    pass
survivors = []
for pid in pids:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        continue
    survivors.append(pid)
pathlib.Path(survivor_path).write_text(
    "".join(f"{pid}\n" for pid in survivors), encoding="utf-8"
)
if pids:
    try:
        os.killpg(pids[0], signal.SIGKILL)
    except ProcessLookupError:
        pass
pathlib.Path(status_path).write_text(f"{status}\n", encoding="utf-8")
pathlib.Path(stderr_path).write_bytes(stderr)
PY
[[ "$(cat "$status_file")" -eq 2 ]] \
  || fail "hung gh was not classified as infrastructure exit 2 (got $(cat "$status_file"))"
grep -q 'deadline' "$stderr_file" || fail "hung gh diagnostic does not name the deadline"
[[ -s "$pid_file" ]] || fail "fake gh did not record its pid"
[[ ! -s "$survivor_file" ]] \
  || fail "hung gh processes survived verifier timeout: $(tr '\n' ' ' <"$survivor_file")"
while IFS= read -r pid; do
  if kill -0 "$pid" 2>/dev/null; then
    fail "hung gh process $pid survived verifier timeout"
  fi
done <"$pid_file"
[[ "$(wc -l <"$pid_file" | tr -d ' ')" -eq 2 ]] \
  || fail "fake gh did not record both parent and descendant pids"

# The ceiling applies while reading and across both streams. Each stream stays
# below 1 MiB, then the fake sleeps so a post-completion check loses to timeout.
cap_gh="$tmp/cap-gh"
cat >"$cap_gh" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$$" >"$FAKE_GH_PID_FILE"
exec python3 - <<'PY'
import sys
import time

sys.stdout.buffer.write(b"o" * (600 * 1024))
sys.stdout.buffer.flush()
sys.stderr.buffer.write(b"e" * (600 * 1024))
sys.stderr.buffer.flush()
time.sleep(30)
PY
SH
chmod +x "$cap_gh"

cap_status_file="$tmp/cap-gh.status"
cap_stderr_file="$tmp/cap-gh.stderr"
cap_pid_file="$tmp/cap-gh.pid"
FAKE_GH_PID_FILE="$cap_pid_file" \
ASSAY_RELEASE_GH_TIMEOUT_SECONDS=2 \
GH="$cap_gh" \
python3 - "$ORACLE" "$cap_status_file" "$cap_stderr_file" "$cap_pid_file" <<'PY'
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
    _, stderr = process.communicate(timeout=5.0)
    status = process.returncode
except subprocess.TimeoutExpired:
    os.killpg(process.pid, signal.SIGKILL)
    _, stderr = process.communicate()
    status = 124
pathlib.Path(status_path).write_text(f"{status}\n", encoding="utf-8")
pathlib.Path(stderr_path).write_bytes(stderr)
try:
    fake_pid = int(pathlib.Path(pid_path).read_text(encoding="utf-8").strip())
    os.killpg(fake_pid, signal.SIGKILL)
except (FileNotFoundError, ProcessLookupError, ValueError):
    pass
PY
[[ "$(cat "$cap_status_file")" -eq 2 ]] \
  || fail "combined output overflow was not infrastructure exit 2 (got $(cat "$cap_status_file"))"
grep -q 'combined stdout+stderr' "$cap_stderr_file" \
  || fail "combined output overflow lacks the size diagnostic"
if grep -q 'deadline' "$cap_stderr_file"; then
  fail "combined output overflow reached the command deadline"
fi
[[ -s "$cap_pid_file" ]] || fail "combined output fake gh did not record its pid"
if kill -0 "$(cat "$cap_pid_file")" 2>/dev/null; then
  fail "combined output fake gh survived the size ceiling"
fi

# Charge before display: the one byte beyond the cap must not reach stderr.
sink_gh="$tmp/sink-gh"
cat >"$sink_gh" <<'SH'
#!/usr/bin/env bash
exec python3 - <<'PY'
import sys
import time

sys.stderr.buffer.write(b"\x01" * (1024 * 1024 + 1))
sys.stderr.buffer.flush()
time.sleep(30)
PY
SH
chmod +x "$sink_gh"

sink_stderr_file="$tmp/sink-gh.stderr"
sink_status=0
ASSAY_RELEASE_GH_TIMEOUT_SECONDS=2 GH="$sink_gh" \
  "$ORACLE" --self-test >/dev/null 2>"$sink_stderr_file" || sink_status=$?
[[ "$sink_status" -eq 2 ]] \
  || fail "stderr overflow was not infrastructure exit 2 (got $sink_status)"
grep -q 'combined stdout+stderr' "$sink_stderr_file" \
  || fail "stderr overflow lacks the size diagnostic"
if grep -q 'deadline' "$sink_stderr_file"; then
  fail "stderr overflow reached the command deadline"
fi
python3 - "$sink_stderr_file" <<'PY'
import pathlib
import sys

visible = pathlib.Path(sys.argv[1]).read_bytes().count(b"\x01")
limit = 1024 * 1024
if visible != limit:
    raise SystemExit(
        f"overflow bytes reached stderr: expected {limit} visible child bytes, got {visible}"
    )
PY

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
