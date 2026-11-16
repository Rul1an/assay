#!/usr/bin/env bash
# Install the last release of assay beside this tree, for the reader comparison in
# crates/assay-cli/tests/docs_corpus_loads.rs (ADR-052).
#
# The release is the one pinned in .github/assay-release-tag, read by its one parser. It is
# installed by the checked-in installer, scripts/install.sh, into a directory of its own, so the
# binary on PATH is not replaced. The installer checks the archive against the checksum the
# release publishes; without cosign on the runner it does not check the signed manifest, and it
# says so. The comparison needs the release's behaviour, not a proof of its origin.
#
# On success it tells the test where the binary is and that the comparison is required:
#   ASSAY_RELEASED_BIN=<path>            the installed binary, which reported the pinned version
#   ASSAY_READER_COMPARISON=required     without the binary the test fails instead of skipping
# to $GITHUB_ENV when that is set, and to stdout as `export` lines when it is not.
#
# Environment (tests only): ASSAY_RELEASED_INSTALLER replaces the installer, and
# ASSAY_RELEASE_TAG_FILE replaces the pin (read-assay-release-tag.sh takes it).
# ASSAY_RELEASED_DIR sets the directory.
#
# `--self-test` runs the script against stub installers and fails unless each wrong outcome
# (another version, no binary, a failing installer, a malformed pin, a binary left over from an
# earlier run) is refused and the right one writes exactly the two lines.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SELF="${ROOT}/scripts/ci/install-released-assay.sh"

fail() {
  echo "install-released-assay: FAIL: $*" >&2
  exit 1
}

install_release() {
  local installer="${ASSAY_RELEASED_INSTALLER:-${ROOT}/scripts/install.sh}"
  local tag dest bin said
  # The parser also writes `version=` to GITHUB_OUTPUT when that is set; this step has no use
  # for an output, so it is kept out of the way.
  tag="$(env -u GITHUB_OUTPUT bash "${ROOT}/scripts/ci/read-assay-release-tag.sh")" \
    || fail "the release pin could not be read"
  dest="${ASSAY_RELEASED_DIR:-${RUNNER_TEMP:-${TMPDIR:-/tmp}}/assay-released-${tag}}"
  # A binary from an earlier run must not stand in for one this run failed to install.
  rm -rf "${dest}"
  mkdir -p "${dest}"
  ASSAY_VERSION="${tag}" ASSAY_INSTALL_DIR="${dest}" sh "${installer}" >&2 \
    || fail "the installer did not install ${tag}"
  bin="${dest}/assay"
  [[ -f "${bin}" && -x "${bin}" ]] || fail "the installer left no executable at ${bin}"
  said="$("${bin}" --version)" || fail "${bin} --version failed"
  [[ "${said}" == "assay ${tag#v}" ]] \
    || fail "the installed binary says '${said}' and the pin says ${tag}"
  if [[ -n "${GITHUB_ENV:-}" ]]; then
    printf 'ASSAY_RELEASED_BIN=%s\nASSAY_READER_COMPARISON=required\n' "${bin}" >>"${GITHUB_ENV}"
  else
    printf 'export ASSAY_RELEASED_BIN=%q\nexport ASSAY_READER_COMPARISON=required\n' "${bin}"
  fi
  echo "install-released-assay: ${tag} installed at ${bin}" >&2
}

self_test() {
  local tmp failed=0
  tmp="$(mktemp -d)"
  trap 'rm -rf "${tmp}"' RETURN

  # A stub installer: writes an `assay` that reports the version in STUB_SAYS, or does what
  # STUB_MODE says instead.
  cat >"${tmp}/installer.sh" <<'STUB'
#!/bin/sh
case "${STUB_MODE:-install}" in
  fail) exit 3 ;;
  nothing) exit 0 ;;
esac
printf '#!/bin/sh\necho "assay %s"\n' "${STUB_SAYS}" >"${ASSAY_INSTALL_DIR}/assay"
chmod 755 "${ASSAY_INSTALL_DIR}/assay"
STUB
  printf 'v9.9.9\n' >"${tmp}/pin"
  printf 'latest\n' >"${tmp}/bad-pin"

  # name @@ expected exit @@ text stderr must contain @@ environment
  local cases=(
    "the pinned version is installed@@0@@v9.9.9 installed at@@STUB_SAYS=9.9.9"
    "the binary reports another version@@1@@and the pin says v9.9.9@@STUB_SAYS=9.9.8"
    "the installer installs nothing@@1@@left no executable@@STUB_MODE=nothing"
    "the installer fails@@1@@did not install v9.9.9@@STUB_MODE=fail"
    "the pin is not a release tag@@1@@could not be read@@STUB_SAYS=9.9.9 ASSAY_RELEASE_TAG_FILE=${tmp}/bad-pin"
  )
  local entry name want text envs rest code err
  for entry in "${cases[@]}"; do
    name="${entry%%@@*}"; rest="${entry#*@@}"
    want="${rest%%@@*}"; rest="${rest#*@@}"
    text="${rest%%@@*}"; envs="${rest#*@@}"
    : >"${tmp}/env"
    # A binary of the right version left in the directory by an earlier run: every case that
    # must fail has to fail in spite of it.
    mkdir -p "${tmp}/dest"
    printf '#!/bin/sh\necho "assay 9.9.9"\n' >"${tmp}/dest/assay"
    chmod 755 "${tmp}/dest/assay"
    set +e
    # shellcheck disable=SC2086
    err="$(env ASSAY_RELEASED_INSTALLER="${tmp}/installer.sh" ASSAY_RELEASE_TAG_FILE="${tmp}/pin" \
      ASSAY_RELEASED_DIR="${tmp}/dest" GITHUB_ENV="${tmp}/env" ${envs} bash "${SELF}" 2>&1 >/dev/null)"
    code=$?
    set -e
    if [[ "${code}" != "${want}" || "${err}" != *"${text}"* ]]; then
      echo "self-test FAIL: ${name}: exit ${code}, wanted ${want} with '${text}': ${err}" >&2
      failed=1
    fi
    if [[ "${want}" == 0 ]]; then
      if [[ "$(cat "${tmp}/env")" != "ASSAY_RELEASED_BIN=${tmp}/dest/assay"$'\n'"ASSAY_READER_COMPARISON=required" ]]; then
        echo "self-test FAIL: ${name}: GITHUB_ENV holds: $(cat "${tmp}/env")" >&2
        failed=1
      fi
    elif [[ -s "${tmp}/env" ]]; then
      echo "self-test FAIL: ${name}: a failed run wrote to GITHUB_ENV: $(cat "${tmp}/env")" >&2
      failed=1
    fi
  done
  [[ ${failed} -eq 0 ]] || return 1
  echo "install-released-assay self-test: ${#cases[@]} cases"
}

if [[ "${1:-}" == "--self-test" ]]; then
  self_test
else
  install_release
fi
