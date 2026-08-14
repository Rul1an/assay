#!/usr/bin/env bash
# Offline contract for scripts/install.sh version normalization (#2375).
#
# Explicit ASSAY_VERSION values 5.2.0 and v5.2.0 must derive the same GitHub
# tag and archive prefix (v5.2.0). latest stays latest. Malformed input must
# fail before any curl invocation. A stub curl records every call; nothing
# reaches the network.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALLER="${INSTALLER:-$ROOT/scripts/install.sh}"
MODE="${1:-}"

abort_is_failure() {
  local rc="$1"
  if [[ "${rc}" -ne 0 ]]; then
    echo "install-version-normalization contract aborted (exit ${rc}); treat as failure" >&2
  fi
}
trap 'abort_is_failure "$?"' ERR

if [[ ! -f "$INSTALLER" ]]; then
  echo "missing installer: $INSTALLER" >&2
  exit 1
fi

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
failures=0

fail() {
  echo "FAIL $*" >&2
  failures=$((failures + 1))
}

ok() {
  echo "ok   $*"
}

make_curl_stub() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"
  cat >"$bin_dir/curl" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'curl %s\n' "$*" >>"${CURL_LOG:?}"
out=""
write_fmt=""
url=""
prev=""
for arg in "$@"; do
  case "$prev" in
    -o) out="$arg" ;;
    -w) write_fmt="$arg" ;;
  esac
  case "$arg" in
    http://*|https://*) url="$arg" ;;
  esac
  prev="$arg"
done
printf '%s\n' "${url:-<missing-url>}" >>"${CURL_URLS:?}"
if [[ "$url" == *"/repos/Rul1an/assay/releases/latest" ]]; then
  payload='{"tag_name":"v9.9.9"}'
  if [[ -n "${LATEST_API_PAYLOAD:-}" ]]; then
    payload="$LATEST_API_PAYLOAD"
  fi
  if [[ -n "$out" ]]; then
    printf '%s\n' "$payload" >"$out"
  else
    printf '%s\n' "$payload"
  fi
  if [[ -n "$write_fmt" ]]; then
    printf '200'
  fi
  exit 0
fi
if [[ -n "$out" ]]; then
  : >"$out"
fi
if [[ -n "$write_fmt" ]]; then
  printf '404'
fi
exit 0
STUB
  chmod +x "$bin_dir/curl"
}

run_installer() {
  local version_mode="$1"
  local run_dir="$2"
  local log="$run_dir/installer.log"
  local urls="$run_dir/urls"
  local curl_log="$run_dir/curl.log"
  mkdir -p "$run_dir"
  : >"$urls"
  : >"$curl_log"
  make_curl_stub "$run_dir/bin"
  local rc=0
  local env_cmd=(env -u ASSAY_VERSION)
  case "$version_mode" in
    unset) ;;
    empty) env_cmd=(env ASSAY_VERSION=) ;;
    *) env_cmd=(env "ASSAY_VERSION=$version_mode") ;;
  esac
  "${env_cmd[@]}" \
    CURL_LOG="$curl_log" CURL_URLS="$urls" \
    ASSAY_INSTALL_DIR="$run_dir/prefix" \
    PATH="$run_dir/bin:/usr/bin:/bin" \
    HOME="$run_dir/home" \
    sh "$INSTALLER" >"$log" 2>&1 || rc=$?
  printf '%s\n' "$rc"
}

expect_same_v_tag_url() {
  local label="$1"
  local version="$2"
  local run_dir="$SCRATCH/url-$label"
  local rc
  rc="$(run_installer "$version" "$run_dir")"
  local urls="$run_dir/urls"
  if [[ ! -s "$urls" ]]; then
    fail "$label: installer never constructed a download URL (exit $rc)"
    sed 's/^/      /' "$run_dir/installer.log" >&2 || true
    return
  fi
  if ! grep -Fq -- "/releases/download/v5.2.0/assay-v5.2.0-" "$urls"; then
    fail "$label: expected tag+archive v5.2.0, got:"
    sed 's/^/      /' "$urls" >&2
    return
  fi
  if grep -Eq -- '/releases/download/5[.]2[.]0/|/assay-5[.]2[.]0-' "$urls"; then
    fail "$label: unprefixed 5.2.0 leaked into tag or archive:"
    sed 's/^/      /' "$urls" >&2
    return
  fi
  ok "$label derives /download/v5.2.0/assay-v5.2.0- (exit $rc)"
}

expect_fail_before_network() {
  local label="$1"
  local version="$2"
  local run_dir="$SCRATCH/deny-$label"
  local rc
  rc="$(run_installer "$version" "$run_dir")"
  if [[ "$rc" -eq 0 ]]; then
    fail "$label: installer exited 0 for malformed ASSAY_VERSION=$version"
    return
  fi
  if [[ -s "$run_dir/urls" ]] || [[ -s "$run_dir/curl.log" ]]; then
    fail "$label: malformed ASSAY_VERSION=$version reached curl before failing"
    sed 's/^/      /' "$run_dir/curl.log" >&2 || true
    sed 's/^/      /' "$run_dir/urls" >&2 || true
    return
  fi
  ok "$label fails closed before network (exit $rc)"
}

expect_latest_path() {
  local label="$1"
  local version="$2"
  local run_dir="$SCRATCH/latest-$label"
  local rc
  rc="$(run_installer "$version" "$run_dir")"
  if ! grep -Fq -- "/repos/Rul1an/assay/releases/latest" "$run_dir/urls"; then
    fail "$label: did not query the latest-release API (exit $rc)"
    sed 's/^/      /' "$run_dir/urls" >&2 || true
    return
  fi
  if grep -Eq -- '/download/vlatest|/download/latest/|/assay-vlatest-' "$run_dir/urls"; then
    fail "$label: acquired a v prefix or was used as a tag"
    sed 's/^/      /' "$run_dir/urls" >&2
    return
  fi
  if ! grep -Fq -- "/releases/download/v9.9.9/assay-v9.9.9-" "$run_dir/urls"; then
    fail "$label: resolved tag/archive were not derived from the API tag"
    sed 's/^/      /' "$run_dir/urls" >&2
    return
  fi
  ok "$label resolves via the latest API (exit $rc)"
}

# A latest-API body with more than one tag_name line extracts to a multiline
# VERSION. Line-oriented grep would treat that as stable and curl the asset
# with the full invalid tag. Fail closed; the latest query is not an asset curl.
expect_latest_rejects_multiline_tags() {
  local label="latest-multiline-tags"
  local payload
  payload=$'{"tag_name":"evil-not-a-tag"}\n{"tag_name":"v9.9.9"}'
  local run_dir="$SCRATCH/latest-multiline"
  local rc
  rc="$(LATEST_API_PAYLOAD="$payload" run_installer "latest" "$run_dir")"
  if [[ "$rc" -eq 0 ]]; then
    fail "$label: installer accepted a multiline latest-API tag (exit 0)"
    sed 's/^/      /' "$run_dir/installer.log" >&2 || true
    return
  fi
  if ! grep -Fq -- "/repos/Rul1an/assay/releases/latest" "$run_dir/urls"; then
    fail "$label: did not query the latest-release API (exit $rc)"
    sed 's/^/      /' "$run_dir/urls" >&2 || true
    return
  fi
  if grep -Fq -- "/releases/download/" "$run_dir/urls"; then
    fail "$label: multiline latest-API tag proceeded to asset curl"
    sed 's/^/      /' "$run_dir/urls" >&2
    return
  fi
  ok "$label fails closed with no asset curl (exit $rc)"
}

expect_cmdsubst_does_not_create_sentinel() {
  local run_dir="$SCRATCH/deny-cmdsubst"
  local sentinel="$run_dir/PWNED"
  # Literal dollar-paren, not an expansion: the installer must not create $sentinel.
  local version
  version="\$(touch ${sentinel})"
  local rc
  rc="$(run_installer "$version" "$run_dir")"
  if [[ "$rc" -eq 0 ]]; then
    fail "cmdsubst: installer exited 0 for a command-substitution string"
    return
  fi
  if [[ -s "$run_dir/urls" ]] || [[ -s "$run_dir/curl.log" ]]; then
    fail "cmdsubst: command-substitution string reached curl"
    sed 's/^/      /' "$run_dir/curl.log" >&2 || true
    return
  fi
  if [[ -e "$sentinel" ]]; then
    fail "cmdsubst: literal \$(touch ...) created sentinel $sentinel"
    return
  fi
  ok "cmdsubst fails closed and does not create a sentinel (exit $rc)"
}

run_contract() {
  expect_same_v_tag_url "unprefixed" "5.2.0"
  expect_same_v_tag_url "prefixed" "v5.2.0"
  expect_latest_path "literal-latest" "latest"
  expect_latest_path "unset" "unset"
  expect_latest_rejects_multiline_tags
  expect_fail_before_network "empty" "empty"
  expect_fail_before_network "bare-v" "v"
  expect_fail_before_network "dotdot" ".."
  expect_fail_before_network "url" "https://example.com/v5.2.0"
  expect_fail_before_network "trailing-space" "5.2.0 "
  expect_fail_before_network "leading-space" " 5.2.0"
  expect_fail_before_network "latest-foo" "latest-foo"
  expect_fail_before_network "double-v" "vv5.2.0"
  expect_fail_before_network "two-part" "5.2"
  expect_fail_before_network "prerelease" "5.2.0-rc.1"
  expect_fail_before_network "percent" "5.2.0%2f"
  expect_fail_before_network "semicolon" "5.2.0;id"
  expect_fail_before_network "newline" $'5.2.0\nextra'
  expect_cmdsubst_does_not_create_sentinel

  if [[ "$INSTALLER" == "$ROOT/scripts/install.sh" ]]; then
    if ! grep -qE '^normalize_install_version[[:space:]]*\(\)' "$INSTALLER"; then
      fail "install.sh does not define normalize_install_version()"
    else
      ok "install.sh defines normalize_install_version()"
    fi
    if ! grep -qE '^is_stable_release_tag[[:space:]]*\(\)' "$INSTALLER"; then
      fail "install.sh does not define is_stable_release_tag()"
    else
      ok "install.sh defines is_stable_release_tag()"
    fi
    if ! grep -Fq -- '^v[0-9]+[.][0-9]+[.][0-9]+$' "$INSTALLER"; then
      fail "install.sh lost the stable-tag regex required by the latest-channel contract"
    else
      ok "stable-tag regex remains the latest-channel literal"
    fi
    if grep -Fq -- "ASSAY_INSTALL_SOURCE_ONLY" "$INSTALLER"; then
      fail "install.sh grew a test-only source/skip escape"
    else
      ok "install.sh has no source-only production escape"
    fi
  fi
}

if [[ "$MODE" == "--no-mutations" ]]; then
  run_contract
  if [[ "$failures" -ne 0 ]]; then
    echo "install-version-normalization contract: ${failures} failure(s)" >&2
    exit 1
  fi
  echo "install-version-normalization contract: all cases passed"
  exit 0
fi

run_contract

echo "== mutations must make this contract fail =="

apply_mutation() {
  local name="$1"
  local mutant="$2"
  python3 - "$INSTALLER" "$mutant" "$name" <<'PY'
import pathlib, sys

src, dest, name = sys.argv[1], sys.argv[2], sys.argv[3]
text = pathlib.Path(src).read_text()
mutations = {
    "unprefixed-archive": (
        'ARCHIVE_NAME="assay-${VERSION}-${TARGET}.tar.gz"',
        'ARCHIVE_NAME="assay-${VERSION#v}-${TARGET}.tar.gz"',
    ),
    "malformed-as-latest": (
        'VERSION="$(normalize_install_version "$VERSION")" || log_error "ASSAY_VERSION must be latest or a stable X.Y.Z (optional leading v)"',
        'if ! VERSION="$(normalize_install_version "$VERSION")"; then VERSION="latest"; fi',
    ),
    "latest-v-prefix": (
        'if [ "$1" = "latest" ]; then\n        printf \'%s\\n\' "latest"',
        'if [ "$1" = "latest" ]; then\n        printf \'%s\\n\' "vlatest"',
    ),
    "drop-tag-prefilter": (
        '    case "$1" in\n        ""|*[[:space:]]*|*/*|*..*)\n            return 1\n            ;;\n    esac\n    printf \'%s\\n\' "$1" | grep -Eq \'^v[0-9]+[.][0-9]+[.][0-9]+$\'',
        '    printf \'%s\\n\' "$1" | grep -Eq \'^v[0-9]+[.][0-9]+[.][0-9]+$\'',
    ),
}
if name not in mutations:
    raise SystemExit(f"unknown mutation {name!r}")
old, new = mutations[name]
count = text.count(old)
if count != 1:
    raise SystemExit(f"mutation {name!r} anchor matched {count} times, expected once")
pathlib.Path(dest).write_text(text.replace(old, new, 1))
PY
}

expect_mutation_bites() {
  local name="$1"
  local mutant="$SCRATCH/mutant-$name.sh"
  apply_mutation "$name" "$mutant"
  chmod +x "$mutant"
  local log="$SCRATCH/mutant-$name.log"
  if INSTALLER="$mutant" bash "$0" --no-mutations >"$log" 2>&1; then
    fail "mutation $name stayed green"
    sed 's/^/      /' "$log" >&2
    return
  fi
  ok "mutation $name bites"
}

if [[ "$failures" -eq 0 ]]; then
  expect_mutation_bites "unprefixed-archive"
  expect_mutation_bites "malformed-as-latest"
  expect_mutation_bites "latest-v-prefix"
  expect_mutation_bites "drop-tag-prefilter"
fi

if [[ "$failures" -ne 0 ]]; then
  echo "install-version-normalization contract: ${failures} failure(s)" >&2
  exit 1
fi
echo "install-version-normalization contract: all cases passed"
