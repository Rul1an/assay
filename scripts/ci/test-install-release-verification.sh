#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALLER="${INSTALLER:-$ROOT/scripts/install.sh}"
HOOK_CHECKER="${HOOK_CHECKER:-$ROOT/scripts/ci/check-install-release-verification-hook.sh}"
TEST_TMP=""

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

assert_precommit_wiring() {
  bash "$HOOK_CHECKER"

  local mutation_dir="$TEST_TMP/hook-mutations"
  mkdir -p "$mutation_dir"
  python3 - "$ROOT/.pre-commit-config.yaml" "$mutation_dir" <<'PY'
import sys
from pathlib import Path

source = Path(sys.argv[1]).read_text(encoding="utf-8")
dest = Path(sys.argv[2])
hook_pos = source.index("      - id: install-release-verification-self-test\n")
owner_pos = source.rfind("  - repo: local\n", 0, hook_pos)
if owner_pos < 0:
    raise SystemExit("test mutation wrong-owner could not find owning repo")
wrong_owner = (
    source[:owner_pos]
    + "  - repo: https://example.invalid/hooks\n"
    + source[owner_pos + len("  - repo: local\n"):]
)
mutations = {
    "wrong-owner": wrong_owner,
    "comment-decoy-entry": source.replace(
        "entry: bash scripts/ci/test-install-release-verification.sh",
        "entry: true # entry: bash scripts/ci/test-install-release-verification.sh",
        1,
    ),
    "empty-selector": source.replace(
        "files: ^(scripts/install\\.sh|scripts/ci/(check-install-release-verification-hook|test-install-release-verification)\\.sh|README\\.md|\\.pre-commit-config\\.yaml)$",
        "files: ^$",
        1,
    ),
    "manual-stage": source.replace(
        "      - id: install-release-verification-self-test\n"
        "        name: installer release verification contract\n"
        "        entry: bash scripts/ci/test-install-release-verification.sh\n"
        "        language: system\n"
        "        pass_filenames: false\n"
        "        stages: [pre-commit]\n",
        "      - id: install-release-verification-self-test\n"
        "        name: installer release verification contract\n"
        "        entry: bash scripts/ci/test-install-release-verification.sh\n"
        "        language: system\n"
        "        pass_filenames: false\n"
        "        stages: [manual]\n",
        1,
    ),
}
for name, text in mutations.items():
    if text == source:
        raise SystemExit(f"test mutation {name} did not apply")
    (dest / f"{name}.yaml").write_text(text, encoding="utf-8")
PY
  local mutated
  for mutated in "$mutation_dir"/*.yaml; do
    if PRECOMMIT_CONFIG="$mutated" bash "$HOOK_CHECKER" >/dev/null 2>&1; then
      fail "hook checker accepted mutation $(basename "$mutated")"
    fi
  done
}

make_fixture() {
  local root="$1"
  local target="$2"
  local version="v5.5.2"
  local archive_name="assay-${version}-${target}.tar.gz"
  local archive="$root/$archive_name"
  local payload_root="$root/payload-$target"
  local payload="$payload_root/assay-${version}-${target}"

  mkdir -p "$payload"
  cat > "$payload/assay" <<'EOF'
#!/bin/sh
echo 'assay 5.5.2 fixture'
EOF
  chmod +x "$payload/assay"
  tar -C "$payload_root" -czf "$archive" "assay-${version}-${target}"
  printf '%s  %s\n' "$(compute_sha256 "$archive")" "$archive_name" > "${archive}.sha256"
}

make_uname_stub() {
  local bin_dir="$1"
  cat > "$bin_dir/uname" <<'EOF'
#!/bin/sh
case "$1" in
  -s) printf '%s\n' "${FAKE_OS:-Linux}" ;;
  -m) printf '%s\n' x86_64 ;;
  *) exit 2 ;;
esac
EOF
  chmod +x "$bin_dir/uname"
}

make_curl_stub() {
  local bin_dir="$1"
  cat > "$bin_dir/curl" <<'EOF'
#!/bin/sh
set -eu

out=""
wants_status=0
max_filesize=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      out="$2"
      shift 2
      ;;
    -w)
      wants_status=1
      shift 2
      ;;
    --max-filesize)
      max_filesize="$2"
      shift 2
      ;;
    -*)
      shift
      ;;
    *)
      url="$1"
      shift
      ;;
  esac
done

printf '%s\n' "$url" >> "$CURL_LOG"
case "$url" in
  *.tar.gz.sha256)
    asset_name="$(basename "${url%.sha256}")"
    fixture_archive="$FIXTURE_DIR/$asset_name"
    fixture_sidecar="${fixture_archive}.sha256"
    printf '%s\n' "${max_filesize:-none}" >> "$CURL_LIMIT_LOG"
    case "${CURL_MODE:-ok}" in
      missing-sidecar) exit 22 ;;
      malformed-sidecar) printf '%s\n' 'not-a-sidecar' > "$out" ;;
      wrong-asset-sidecar)
        digest="$(cut -d' ' -f1 "$fixture_sidecar")"
        printf '%s  %s\n' "$digest" 'assay-v5.5.2-aarch64-unknown-linux-gnu.tar.gz' > "$out"
        ;;
      trailing-garbage)
        cat "$fixture_sidecar" > "$out"
        printf '%s' 'garbage' >> "$out"
        ;;
      oversized-sidecar)
        if [ -n "$max_filesize" ] && [ "$max_filesize" -lt 4096 ]; then
          dd if=/dev/zero bs=1 count="$max_filesize" 2>/dev/null | tr '\000' a > "$out"
          exit 63
        fi
        dd if=/dev/zero bs=1 count=4096 2>/dev/null | tr '\000' a > "$out"
        ;;
      mismatch)
        printf '%064d  %s\n' 0 "$asset_name" > "$out"
        ;;
      *) cp "$fixture_sidecar" "$out" ;;
    esac
    ;;
  *.tar.gz)
    cp "$FIXTURE_DIR/$(basename "$url")" "$out"
    if [ "${CURL_MODE:-ok}" = block-after-archive ]; then
      : > "$SIGNAL_MARKER"
      sleep 2
    fi
    ;;
  *)
    echo "unexpected curl URL: $url" >&2
    exit 2
    ;;
esac

if [ "$wants_status" -eq 1 ]; then
  printf '%s' 200
fi
EOF
  chmod +x "$bin_dir/curl"
}

make_gh_stub() {
  local bin_dir="$1"
  cat > "$bin_dir/gh" <<'EOF'
#!/bin/sh
set -eu
{
  printf '%s\n' '--- invocation ---'
  printf '%s\n' "$@"
} >> "$GH_LOG"

if [ "$1" = api ]; then
  case "$2" in
    repos/Rul1an/assay/git/ref/tags/v5.5.2)
      if [ "${GH_TAG_MODE:-ok}" = invalid-digest ]; then
        printf '%s\n' 'commit aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
      else
        printf '%s\n' 'tag aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'
      fi
      ;;
    repos/Rul1an/assay/git/tags/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa)
      printf '%s\n' 'commit bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'
      ;;
    *)
      echo "unexpected gh api request: $2" >&2
      exit 2
      ;;
  esac
  exit 0
fi

if [ "$1" = attestation ] && [ "$2" = verify ]; then
  if [ "${GH_VERIFY_MODE:-ok}" = fail ]; then
    echo 'simulated attestation refusal' >&2
    exit 1
  fi
  exit 0
fi

echo "unexpected gh invocation: $*" >&2
exit 2
EOF
  chmod +x "$bin_dir/gh"
}

make_chmod_stub() {
  local bin_dir="$1"
  cat > "$bin_dir/chmod" <<'EOF'
#!/bin/sh
if [ "${CHMOD_MODE:-ok}" = fail ]; then
  exit 1
fi
exec /bin/chmod "$@"
EOF
  chmod +x "$bin_dir/chmod"
}

new_case() {
  local name="$1"
  local case_dir="$TEST_TMP/$name"
  mkdir -p "$case_dir/bin" "$case_dir/home" "$case_dir/install" "$case_dir/tmp"
  make_uname_stub "$case_dir/bin"
  make_curl_stub "$case_dir/bin"
  make_gh_stub "$case_dir/bin"
  make_chmod_stub "$case_dir/bin"
  printf '%s\n' 'old binary' > "$case_dir/install/assay"
  printf '%s\n' "$case_dir"
}

run_installer() {
  local case_dir="$1"
  shift
  env \
    HOME="$case_dir/home" \
    PATH="$case_dir/bin:/usr/bin:/bin" \
    ASSAY_VERSION=5.5.2 \
    ASSAY_INSTALL_DIR="$case_dir/install" \
    FIXTURE_DIR="$TEST_TMP" \
    CURL_LOG="$case_dir/curl.log" \
    CURL_LIMIT_LOG="$case_dir/curl-limit.log" \
    GH_LOG="$case_dir/gh.log" \
    "$@" \
    sh "$INSTALLER"
}

assert_old_binary() {
  local case_dir="$1"
  grep -Fx 'old binary' "$case_dir/install/assay" >/dev/null || \
    fail "verification failure replaced the existing binary in $case_dir"
}

file_mode() {
  if stat -f '%Lp' "$1" >/dev/null 2>&1; then
    stat -f '%Lp' "$1"
  else
    stat -c '%a' "$1"
  fi
}

assert_exact_gh_invocation() {
  local log="$1"
  shift
  python3 - "$log" "$@" <<'PY'
import sys
from pathlib import Path

lines = Path(sys.argv[1]).read_text(encoding="utf-8").splitlines()
expected = sys.argv[2:]
invocations = []
current = None
for line in lines:
    if line == "--- invocation ---":
        if current is not None:
            invocations.append(current)
        current = []
    elif current is not None:
        current.append(line)
if current is not None:
    invocations.append(current)
def matches(actual):
    if len(actual) != len(expected):
        return False
    for observed, wanted in zip(actual, expected):
        if wanted == "<ARCHIVE>":
            if Path(observed).name != "assay-v5.5.2-x86_64-unknown-linux-gnu.tar.gz":
                return False
        elif observed != wanted:
            return False
    return True

attestation_invocations = [
    invocation for invocation in invocations
    if invocation[:2] == ["attestation", "verify"]
]
match_count = sum(matches(invocation) for invocation in attestation_invocations)
if match_count != 1 or len(attestation_invocations) != 1:
    raise SystemExit(
        f"expected exactly one gh invocation {expected!r}, found {match_count}; "
        f"observed={invocations!r}"
    )
PY
}

assert_extra_attestation_invocation_is_rejected() {
  local source_log="$1"
  local extra_log="$2"
  cp "$source_log" "$extra_log"
  cat >> "$extra_log" <<'EOF'
--- invocation ---
attestation
verify
/tmp/evil.tar.gz
--repo
evil/repo
--signer-workflow
evil/workflow
EOF
  if assert_exact_gh_invocation "$extra_log" \
    attestation verify '<ARCHIVE>' \
    --repo Rul1an/assay \
    --signer-workflow Rul1an/assay/.github/workflows/release.yml \
    --cert-oidc-issuer https://token.actions.githubusercontent.com \
    --predicate-type https://slsa.dev/provenance/v1 \
    --source-digest bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb \
    --deny-self-hosted-runners >/dev/null 2>&1; then
    fail 'exact gh invocation guard accepted an additional attestation call'
  fi
}

assert_default_success() {
  local os="$1"
  local label="$2"
  local target archive_name expected_sidecar_bytes
  local case_dir
  case_dir="$(new_case "default-success-$label")"
  run_installer "$case_dir" FAKE_OS="$os" > "$case_dir/stdout" 2> "$case_dir/stderr"
  grep -F 'checksum_verified' "$case_dir/stdout" >/dev/null || fail 'default install did not report checksum_verified'
  grep -F 'provenance_not_requested' "$case_dir/stdout" >/dev/null || fail 'default install did not report provenance_not_requested'
  if grep -F 'provenance_verified' "$case_dir/stdout" >/dev/null; then
    fail 'checksum-only install claimed provenance_verified'
  fi
  grep -F 'assay 5.5.2 fixture' "$case_dir/install/assay" >/dev/null || fail 'default install did not activate fixture'
  test "$(file_mode "$case_dir/install/assay")" = 755 || fail 'activated binary mode is not exactly 0755'
  test "$(wc -l < "$case_dir/curl.log" | tr -d ' ')" -eq 2 || fail 'default install did not fetch exactly archive and sidecar'
  case "$os" in
    Linux) target=x86_64-unknown-linux-gnu ;;
    Darwin) target=x86_64-apple-darwin ;;
    *) fail "test bug: unsupported fixture OS $os" ;;
  esac
  archive_name="assay-v5.5.2-${target}.tar.gz"
  expected_sidecar_bytes=$((64 + 2 + ${#archive_name} + 1))
  grep -Fx "$expected_sidecar_bytes" "$case_dir/curl-limit.log" >/dev/null || \
    fail 'sidecar download did not apply its exact pre-materialization byte ceiling'
  test ! -s "$case_dir/gh.log" || fail 'default install invoked gh'
}

assert_relative_install_dir_resolves_from_invocation_directory() {
  local case_dir
  case_dir="$(new_case relative-install-dir)"
  (
    cd "$case_dir"
    run_installer "$case_dir" ASSAY_INSTALL_DIR=relative-install > "$case_dir/stdout" 2> "$case_dir/stderr"
  )
  grep -F 'assay 5.5.2 fixture' "$case_dir/relative-install/assay" >/dev/null || \
    fail 'relative ASSAY_INSTALL_DIR did not resolve from the invocation directory'
}

assert_activation_failure_preserves_binary() {
  local case_dir
  case_dir="$(new_case activation-failure)"
  if run_installer "$case_dir" CHMOD_MODE=fail > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail 'installer succeeded after candidate preparation failed'
  fi
  assert_old_binary "$case_dir"
  if find "$case_dir/install" -name '.assay.install.*' -print -quit | grep -q .; then
    fail 'failed activation left a same-directory candidate behind'
  fi
}

assert_signal_stops_before_next_network_step() {
  local signal="$1"
  local expected_rc="$2"
  local label="$3"
  local case_dir marker rc
  case_dir="$(new_case "signal-$label")"
  marker="$case_dir/archive-downloaded"

  env \
    HOME="$case_dir/home" \
    PATH="$case_dir/bin:/usr/bin:/bin" \
    ASSAY_VERSION=5.5.2 \
    ASSAY_INSTALL_DIR="$case_dir/install" \
    FIXTURE_DIR="$TEST_TMP" \
    CURL_LOG="$case_dir/curl.log" \
    CURL_LIMIT_LOG="$case_dir/curl-limit.log" \
    GH_LOG="$case_dir/gh.log" \
    CURL_MODE=block-after-archive \
    SIGNAL_MARKER="$marker" \
    TMPDIR="$case_dir/tmp" \
    python3 - "$INSTALLER" "$marker" "$case_dir/stdout" "$case_dir/stderr" "$case_dir/rc" "$signal" <<'PY'
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

installer, marker, stdout_path, stderr_path, rc_path, signal_name = sys.argv[1:]
with open(stdout_path, "wb") as stdout, open(stderr_path, "wb") as stderr:
    proc = subprocess.Popen(["sh", installer], stdout=stdout, stderr=stderr, env=os.environ.copy())
    for _ in range(100):
        if Path(marker).is_file():
            break
        time.sleep(0.02)
    else:
        proc.kill()
        proc.wait()
        raise SystemExit("signal test did not reach the bounded archive-download witness")
    os.kill(proc.pid, getattr(signal, f"SIG{signal_name}"))
    rc = proc.wait(timeout=10)
Path(rc_path).write_text(f"{rc}\n", encoding="utf-8")
PY
  rc="$(cat "$case_dir/rc")"
  test "$rc" -eq "$expected_rc" || fail "$signal exited $rc, expected $expected_rc"
  assert_old_binary "$case_dir"
  test "$(wc -l < "$case_dir/curl.log" | tr -d ' ')" -eq 1 || \
    fail "$signal allowed the installer to continue to another network step"
  if find "$case_dir/tmp" -mindepth 1 -print -quit | grep -q .; then
    fail "$signal left installer scratch residue"
  fi
}

assert_checksum_failure_preserves_binary() {
  local mode="$1"
  local expected_error
  case "$mode" in
    mismatch) expected_error='Archive checksum mismatch' ;;
    missing-sidecar|oversized-sidecar) expected_error='Download failed' ;;
    malformed-sidecar|wrong-asset-sidecar|trailing-garbage) expected_error='Checksum sidecar must contain exactly one newline-terminated record' ;;
    *) fail "test bug: no expected error for checksum mode $mode" ;;
  esac
  local case_dir
  case_dir="$(new_case "checksum-$mode")"
  if run_installer "$case_dir" CURL_MODE="$mode" > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail "$mode sidecar unexpectedly installed"
  fi
  if ! grep -F "$expected_error" "$case_dir/stdout" "$case_dir/stderr" >/dev/null; then
    cat "$case_dir/stdout" "$case_dir/stderr" >&2
    fail "$mode failed for an unrelated reason"
  fi
  assert_old_binary "$case_dir"
  test ! -s "$case_dir/gh.log" || fail "$mode sidecar invoked gh"
}

assert_invalid_provenance_mode_refuses_before_network() {
  local value="$1"
  local label="$2"
  local case_dir
  case_dir="$(new_case "invalid-provenance-$label")"
  if run_installer "$case_dir" ASSAY_REQUIRE_PROVENANCE="$value" > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail "ASSAY_REQUIRE_PROVENANCE=$label unexpectedly installed"
  fi
  assert_old_binary "$case_dir"
  test ! -s "$case_dir/curl.log" || fail "ASSAY_REQUIRE_PROVENANCE=$label reached curl"
  test ! -s "$case_dir/gh.log" || fail "ASSAY_REQUIRE_PROVENANCE=$label reached gh"
}

assert_missing_verifier_refuses() {
  local case_dir
  case_dir="$(new_case strict-missing-gh)"
  rm "$case_dir/bin/gh"
  if run_installer "$case_dir" ASSAY_REQUIRE_PROVENANCE=1 > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail 'strict mode installed without gh'
  fi
  assert_old_binary "$case_dir"
}

assert_strict_success() {
  local case_dir
  case_dir="$(new_case strict-success)"
  run_installer "$case_dir" ASSAY_REQUIRE_PROVENANCE=1 > "$case_dir/stdout" 2> "$case_dir/stderr"
  grep -F 'checksum_verified' "$case_dir/stdout" >/dev/null || fail 'strict install did not report checksum_verified'
  grep -F 'provenance_verified' "$case_dir/stdout" >/dev/null || fail 'strict install did not report provenance_verified'
  if grep -F 'provenance_not_requested' "$case_dir/stdout" >/dev/null; then
    fail 'strict install reported provenance_not_requested'
  fi

  assert_exact_gh_invocation "$case_dir/gh.log" \
    attestation \
    verify \
    '<ARCHIVE>' \
    --repo \
    Rul1an/assay \
    --signer-workflow \
    Rul1an/assay/.github/workflows/release.yml \
    --cert-oidc-issuer \
    https://token.actions.githubusercontent.com \
    --predicate-type \
    https://slsa.dev/provenance/v1 \
    --source-digest \
    bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb \
    --deny-self-hosted-runners
}

assert_strict_failure_preserves_binary() {
  local case_dir
  case_dir="$(new_case strict-refusal)"
  if run_installer "$case_dir" ASSAY_REQUIRE_PROVENANCE=1 GH_VERIFY_MODE=fail > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail 'strict mode installed after attestation refusal'
  fi
  assert_old_binary "$case_dir"
  if grep -F 'provenance_verified' "$case_dir/stdout" >/dev/null; then
    fail 'strict refusal reported provenance_verified'
  fi
}

assert_invalid_tag_digest_refuses() {
  local case_dir
  case_dir="$(new_case strict-invalid-tag-digest)"
  if run_installer "$case_dir" ASSAY_REQUIRE_PROVENANCE=1 GH_TAG_MODE=invalid-digest > "$case_dir/stdout" 2> "$case_dir/stderr"; then
    fail 'strict mode installed with an invalid tag object digest'
  fi
  assert_old_binary "$case_dir"
  if grep -F 'attestation verify' "$case_dir/gh.log" >/dev/null; then
    fail 'invalid tag object digest reached attestation verification'
  fi
}

main() {
  TEST_TMP="$(mktemp -d)"
  trap 'rm -rf "$TEST_TMP"' EXIT
  assert_precommit_wiring
  make_fixture "$TEST_TMP" x86_64-unknown-linux-gnu
  make_fixture "$TEST_TMP" x86_64-apple-darwin

  assert_default_success Linux linux
  assert_default_success Darwin macos
  for mode in mismatch missing-sidecar malformed-sidecar wrong-asset-sidecar trailing-garbage oversized-sidecar; do
    assert_checksum_failure_preserves_binary "$mode"
  done
  assert_invalid_provenance_mode_refuses_before_network 0 zero
  assert_invalid_provenance_mode_refuses_before_network '' empty
  assert_invalid_provenance_mode_refuses_before_network yes yes
  assert_missing_verifier_refuses
  assert_strict_success
  assert_extra_attestation_invocation_is_rejected \
    "$TEST_TMP/strict-success/gh.log" \
    "$TEST_TMP/strict-success/gh-extra.log"
  assert_strict_failure_preserves_binary
  assert_invalid_tag_digest_refuses
  assert_signal_stops_before_next_network_step HUP 129 hup
  assert_signal_stops_before_next_network_step INT 130 int
  assert_signal_stops_before_next_network_step TERM 143 term
  assert_activation_failure_preserves_binary
  assert_relative_install_dir_resolves_from_invocation_directory

  echo 'install release verification: PASS'
}

main "$@"
