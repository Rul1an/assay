#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALLER="${INSTALLER:-$ROOT/scripts/install.sh}"
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
  python3 - "$ROOT/.pre-commit-config.yaml" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
hook_id = "install-release-verification-self-test"
matches = list(re.finditer(rf"^\s*- id: {re.escape(hook_id)}\s*$", text, re.MULTILINE))
if len(matches) != 1:
    raise SystemExit(f"expected exactly one {hook_id} hook, found {len(matches)}")

start = matches[0].start()
next_hook = re.search(r"^\s*- id: ", text[matches[0].end():], re.MULTILINE)
end = matches[0].end() + next_hook.start() if next_hook else len(text)
block = text[start:end]

expected_entry = "entry: bash scripts/ci/test-install-release-verification.sh"
if expected_entry not in block:
    raise SystemExit(f"{hook_id} does not invoke the contract test")
if not re.search(r"^\s*pass_filenames:\s*false\s*$", block, re.MULTILINE):
    raise SystemExit(f"{hook_id} must set pass_filenames: false")

files_match = re.search(r"^\s*files:\s*(.+?)\s*$", block, re.MULTILINE)
if files_match is None:
    raise SystemExit(f"{hook_id} has no files selector")
selector = re.compile(files_match.group(1))
required = (
    "scripts/install.sh",
    "scripts/ci/test-install-release-verification.sh",
    "README.md",
    ".pre-commit-config.yaml",
)
missed = [candidate for candidate in required if selector.fullmatch(candidate) is None]
if missed:
    raise SystemExit(f"{hook_id} files selector misses: {', '.join(missed)}")
PY
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
    case "${CURL_MODE:-ok}" in
      missing-sidecar) exit 22 ;;
      malformed-sidecar) printf '%s\n' 'not-a-sidecar' > "$out" ;;
      wrong-asset-sidecar)
        digest="$(cut -d' ' -f1 "$fixture_sidecar")"
        printf '%s  %s\n' "$digest" 'assay-v5.5.2-aarch64-unknown-linux-gnu.tar.gz' > "$out"
        ;;
      mismatch)
        printf '%064d  %s\n' 0 "$asset_name" > "$out"
        ;;
      *) cp "$fixture_sidecar" "$out" ;;
    esac
    ;;
  *.tar.gz)
    cp "$FIXTURE_DIR/$(basename "$url")" "$out"
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
printf '%s\n' "$*" >> "$GH_LOG"

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
  mkdir -p "$case_dir/bin" "$case_dir/home" "$case_dir/install"
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
    GH_LOG="$case_dir/gh.log" \
    "$@" \
    sh "$INSTALLER"
}

assert_old_binary() {
  local case_dir="$1"
  grep -Fx 'old binary' "$case_dir/install/assay" >/dev/null || \
    fail "verification failure replaced the existing binary in $case_dir"
}

assert_default_success() {
  local os="$1"
  local label="$2"
  local case_dir
  case_dir="$(new_case "default-success-$label")"
  run_installer "$case_dir" FAKE_OS="$os" > "$case_dir/stdout" 2> "$case_dir/stderr"
  grep -F 'checksum_verified' "$case_dir/stdout" >/dev/null || fail 'default install did not report checksum_verified'
  grep -F 'provenance_not_requested' "$case_dir/stdout" >/dev/null || fail 'default install did not report provenance_not_requested'
  if grep -F 'provenance_verified' "$case_dir/stdout" >/dev/null; then
    fail 'checksum-only install claimed provenance_verified'
  fi
  grep -F 'assay 5.5.2 fixture' "$case_dir/install/assay" >/dev/null || fail 'default install did not activate fixture'
  test "$(wc -l < "$case_dir/curl.log" | tr -d ' ')" -eq 2 || fail 'default install did not fetch exactly archive and sidecar'
  test ! -s "$case_dir/gh.log" || fail 'default install invoked gh'
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

assert_checksum_failure_preserves_binary() {
  local mode="$1"
  local expected_error
  case "$mode" in
    mismatch) expected_error='Archive checksum mismatch' ;;
    missing-sidecar) expected_error='Download failed' ;;
    malformed-sidecar|wrong-asset-sidecar) expected_error='Checksum sidecar does not name the selected archive' ;;
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

  local log="$case_dir/gh.log"
  grep -F 'api repos/Rul1an/assay/git/ref/tags/v5.5.2' "$log" >/dev/null
  grep -F 'api repos/Rul1an/assay/git/tags/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa' "$log" >/dev/null
  grep -F 'attestation verify' "$log" >/dev/null
  grep -F -- '--repo Rul1an/assay' "$log" >/dev/null
  grep -F -- '--signer-workflow Rul1an/assay/.github/workflows/release.yml' "$log" >/dev/null
  grep -F -- '--cert-oidc-issuer https://token.actions.githubusercontent.com' "$log" >/dev/null
  grep -F -- '--predicate-type https://slsa.dev/provenance/v1' "$log" >/dev/null
  grep -F -- '--source-digest bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb' "$log" >/dev/null
  grep -F -- '--deny-self-hosted-runners' "$log" >/dev/null
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
  assert_precommit_wiring
  TEST_TMP="$(mktemp -d)"
  trap 'rm -rf "$TEST_TMP"' EXIT
  make_fixture "$TEST_TMP" x86_64-unknown-linux-gnu
  make_fixture "$TEST_TMP" x86_64-apple-darwin

  assert_default_success Linux linux
  assert_default_success Darwin macos
  for mode in mismatch missing-sidecar malformed-sidecar wrong-asset-sidecar; do
    assert_checksum_failure_preserves_binary "$mode"
  done
  assert_invalid_provenance_mode_refuses_before_network 0 zero
  assert_invalid_provenance_mode_refuses_before_network '' empty
  assert_invalid_provenance_mode_refuses_before_network yes yes
  assert_missing_verifier_refuses
  assert_strict_success
  assert_strict_failure_preserves_binary
  assert_invalid_tag_digest_refuses
  assert_activation_failure_preserves_binary

  echo 'install release verification: PASS'
}

main "$@"
