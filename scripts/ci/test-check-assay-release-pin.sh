#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="${ROOT}/scripts/ci/check-assay-release-pin.sh"
scratch="$(mktemp -d)"
trap 'rm -rf "${scratch}"' EXIT

manifest="${scratch}/Cargo.toml"
pin_file="${scratch}/assay-release-tag"
metadata="${scratch}/release.json"
fake_gh="${scratch}/gh"

write_manifest() {
  printf '[workspace.package]\nversion = "%s"\n' "$1" >"${manifest}"
}

write_pin() {
  printf '%s\n' "$1" >"${pin_file}"
}

write_release() {
  local tag="$1"
  local asset="${2:-assay-${tag}-x86_64-unknown-linux-gnu.tar.gz}"
  local checksum="${3:-${asset}.sha256}"
  cat >"${metadata}" <<EOF
{"tag_name":"${tag}","draft":false,"prerelease":false,"assets":[{"name":"${asset}"},{"name":"${checksum}"}]}
EOF
}

run_check() {
  ASSAY_WORKSPACE_MANIFEST="${manifest}" \
    ASSAY_RELEASE_TAG_FILE="${pin_file}" \
    ASSAY_RELEASE_METADATA_FILE="${metadata}" \
    "${CHECKER}" "$@"
}

run_api_check() {
  ASSAY_WORKSPACE_MANIFEST="${manifest}" \
    ASSAY_RELEASE_TAG_FILE="${pin_file}" \
    ASSAY_GH_BIN="${fake_gh}" \
    "${CHECKER}" --published
}

expect_fail() {
  local expected="$1"
  shift
  if "$@" >"${scratch}/out" 2>"${scratch}/err"; then
    echo "expected failure containing: ${expected}" >&2
    exit 1
  fi
  if ! grep -Fq -- "${expected}" "${scratch}/err"; then
    echo "failure did not contain '${expected}':" >&2
    cat "${scratch}/err" >&2
    exit 1
  fi
}

echo "== offline steady state =="
write_manifest "5.1.0"
write_pin "v5.1.0"
run_check

echo "== offline release preparation may trail workspace =="
write_manifest "5.2.0"
write_pin "v5.1.0"
run_check

echo "== offline pin may not lead workspace =="
write_manifest "5.1.0"
write_pin "v5.2.0"
expect_fail "install pin v5.2.0 leads workspace version 5.1.0" run_check

echo "== published steady state =="
write_manifest "5.1.0"
write_pin "v5.1.0"
write_release "v5.1.0"
run_check --published

echo "== unpublished release pin fails distinctly =="
write_manifest "5.2.0"
write_pin "v5.2.0"
write_release "v5.1.0"
expect_fail "install pin v5.2.0 leads latest published release v5.1.0" run_check --published

echo "== forgotten post-publish pin fails distinctly =="
write_manifest "5.2.0"
write_pin "v5.1.0"
write_release "v5.2.0"
expect_fail "install pin v5.1.0 trails latest published release v5.2.0" run_check --published

echo "== published pin must match the literal release tag =="
write_manifest "5.1.0"
write_pin "v05.1.0"
write_release "v5.1.0"
expect_fail "install pin v05.1.0 does not exactly match latest published release v5.1.0" run_check --published

echo "== draft release metadata fails closed =="
write_pin "v5.1.0"
cat >"${metadata}" <<'EOF'
{"tag_name":"v5.1.0","draft":true,"prerelease":false,"assets":[{"name":"assay-v5.1.0-x86_64-unknown-linux-gnu.tar.gz"}]}
EOF
expect_fail "latest published release v5.1.0 is draft or prerelease" run_check --published

echo "== invalid published tag fails closed =="
write_release "latest" "assay-latest-x86_64-unknown-linux-gnu.tar.gz"
expect_fail "latest published release has an invalid stable tag: 'latest'" run_check --published

echo "== prerelease workspace version fails closed =="
write_manifest "5.2.0-rc.1"
write_pin "v5.1.0"
expect_fail "workspace version is not stable semver: 5.2.0-rc.1" run_check

echo "== missing install asset fails closed =="
write_manifest "5.2.0"
write_pin "v5.2.0"
write_release "v5.2.0" \
  "assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256" \
  "unrelated.txt"
expect_fail "latest published release v5.2.0 lacks assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz" run_check --published

echo "== missing checksum sidecar fails closed =="
write_release "v5.2.0" \
  "assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz" \
  "unrelated.txt"
expect_fail "latest published release v5.2.0 lacks assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256" run_check --published

echo "== unavailable release metadata fails closed =="
rm -f "${metadata}"
expect_fail "failed to obtain latest published release metadata" run_check --published

echo "== failed GitHub API call fails closed =="
cat >"${fake_gh}" <<'EOF'
#!/usr/bin/env bash
exit 71
EOF
chmod +x "${fake_gh}"
expect_fail "failed to obtain latest published release metadata for Rul1an/assay" run_api_check

echo "== fork CI still queries the authoritative upstream release =="
write_manifest "5.1.0"
write_pin "v5.1.0"
write_release "v5.1.0"
cat >"${fake_gh}" <<'EOF'
#!/usr/bin/env bash
if [[ "$*" != "api repos/Rul1an/assay/releases/latest" ]]; then
  exit 72
fi
cat "${ASSAY_RELEASE_METADATA_FIXTURE}"
EOF
chmod +x "${fake_gh}"
GITHUB_REPOSITORY="contributor/assay" \
  ASSAY_RELEASE_METADATA_FIXTURE="${metadata}" \
  run_api_check

echo "== oversized release metadata fails before parsing =="
python3 - "${metadata}" <<'PY'
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(b"x" * (1048576 + 1))
PY
expect_fail "latest published release metadata exceeds 1048576-byte limit" run_check --published

echo "assay release pin contract: PASS"
