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

# Mutation proof: treating expected 404s as failures must make the targeted
# assertion red. This checks behavior through a copied oracle, not a mock.
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
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

printf 'PASS: verify-release offline contract tests\n'
