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
  cat >"${metadata}" <<EOF
{"tag_name":"${tag}","draft":false,"prerelease":false,"assets":[{"name":"${asset}"}]}
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
    GITHUB_REPOSITORY="Rul1an/assay" \
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

echo "== missing install asset fails closed =="
write_pin "v5.2.0"
write_release "v5.2.0" "unrelated.txt"
expect_fail "latest published release v5.2.0 lacks assay-v5.2.0-x86_64-unknown-linux-gnu.tar.gz" run_check --published

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

echo "== oversized release metadata fails before parsing =="
python3 - "${metadata}" <<'PY'
import sys
from pathlib import Path

Path(sys.argv[1]).write_bytes(b"x" * (1048576 + 1))
PY
expect_fail "latest published release metadata exceeds 1048576-byte limit" run_check --published

echo "assay release pin contract: PASS"
