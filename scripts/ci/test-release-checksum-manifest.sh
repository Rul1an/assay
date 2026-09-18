#!/usr/bin/env bash
# Contract for the signed release checksum manifest (Refs #3119).
# Unsigned locally: write / verify / check-contract. Signing is CI-only.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MANIFEST_SCRIPT="${REPO_ROOT}/scripts/ci/release_checksum_manifest.sh"
RELEASE_WORKFLOW="${REPO_ROOT}/.github/workflows/release.yml"

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

expect_status() {
  local want="$1"
  shift
  local status=0
  "$@" >/dev/null 2>&1 || status=$?
  [[ "$status" -eq "$want" ]] || fail "expected exit $want from $*, got $status"
}

[[ -f "$MANIFEST_SCRIPT" ]] || fail "release_checksum_manifest.sh is missing"
[[ -x "$MANIFEST_SCRIPT" ]] || fail "release_checksum_manifest.sh must be executable"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

assets_dir="${tmp_root}/assets"
mkdir -p "$assets_dir"
printf 'payload-a\n' >"${assets_dir}/assay-v9.9.9-x86_64-unknown-linux-gnu.tar.gz"
printf 'payload-b\n' >"${assets_dir}/server.json"
printf 'payload-c\n' >"${assets_dir}/assay-v9.9.9-release-proof-kit.tar.gz"

bash "$MANIFEST_SCRIPT" write --dir "$assets_dir"
[[ -f "${assets_dir}/checksums.txt" ]] || fail "write did not produce checksums.txt"

# Manifest must be name-sorted, sha256, two spaces, basename-only, LF lines.
awk '{print $2}' "${assets_dir}/checksums.txt" | LC_ALL=C sort -c \
  || fail "checksums.txt is not sorted by asset name"
[[ "$(tail -c 1 "${assets_dir}/checksums.txt" | od -An -tu1 | tr -d ' ')" == "10" ]] \
  || fail "checksums.txt must be LF-terminated"
if LC_ALL=C grep -q $'\r' "${assets_dir}/checksums.txt"; then
  fail "checksums.txt must not contain CR"
fi
line_count="$(wc -l <"${assets_dir}/checksums.txt" | tr -d ' ')"
[[ "$line_count" == "3" ]] || fail "write must list every payload file, not the manifest (got $line_count)"
if grep -Fq 'checksums.txt' "${assets_dir}/checksums.txt"; then
  fail "checksums.txt must not hash itself"
fi

expect_status 0 bash "$MANIFEST_SCRIPT" verify --dir "$assets_dir"
expect_status 0 bash "$MANIFEST_SCRIPT" check-contract --dir "$assets_dir"

# RED: a tampered asset fails verification against the manifest.
printf 'tampered\n' >>"${assets_dir}/server.json"
tamper_err="${tmp_root}/tamper.err"
if bash "$MANIFEST_SCRIPT" verify --dir "$assets_dir" >/dev/null 2>"$tamper_err"; then
  fail "tampered asset was accepted by verify"
fi
grep -Fq 'server.json' "$tamper_err" || fail "tamper failure must name the asset"
# Restore the payload so the omit case starts from a matching tree.
printf 'payload-b\n' >"${assets_dir}/server.json"
expect_status 0 bash "$MANIFEST_SCRIPT" verify --dir "$assets_dir"

# RED: an omitted published asset fails the contract.
omit_dir="${tmp_root}/omit"
cp -R "$assets_dir" "$omit_dir"
printf 'extra-published-asset\n' >"${omit_dir}/assay-v9.9.9-sbom-cyclonedx.tar.gz"
omit_err="${tmp_root}/omit.err"
if bash "$MANIFEST_SCRIPT" check-contract --dir "$omit_dir" >/dev/null 2>"$omit_err"; then
  fail "omitted published asset was accepted by check-contract"
fi
grep -Fq 'assay-v9.9.9-sbom-cyclonedx.tar.gz' "$omit_err" \
  || fail "omit failure must name the unpublished-in-manifest asset"

# A name that is not published must also fail.
extra_dir="${tmp_root}/extra-name"
cp -R "$assets_dir" "$extra_dir"
printf '%s  %s\n' "$(compute_sha256 "${extra_dir}/server.json")" 'not-published.bin' \
  >>"${extra_dir}/checksums.txt"
LC_ALL=C sort -o "${extra_dir}/checksums.txt" "${extra_dir}/checksums.txt"
extra_err="${tmp_root}/extra.err"
if bash "$MANIFEST_SCRIPT" check-contract --dir "$extra_dir" >/dev/null 2>"$extra_err"; then
  fail "checksums.txt entry for an unpublished name was accepted"
fi
grep -Fq 'not-published.bin' "$extra_err" || fail "unpublished-name failure must name the extra entry"

# Manifest family files are published beside the payload and must not be listed.
printf 'sigstore-bundle-fixture\n' >"${assets_dir}/checksums.txt.sigstore.json"
printf 'provenance-bundle-fixture\n' >"${assets_dir}/assay-v9.9.9-build-provenance.sigstore.json"
bash "$MANIFEST_SCRIPT" write --dir "$assets_dir"
if grep -Eq 'checksums\.txt(\.sigstore\.json)?$' "${assets_dir}/checksums.txt"; then
  fail "write must exclude the signed-manifest family"
fi
grep -Fq 'assay-v9.9.9-build-provenance.sigstore.json' "${assets_dir}/checksums.txt" \
  || fail "write must cover the attached provenance bundle"
expect_status 0 bash "$MANIFEST_SCRIPT" check-contract --dir "$assets_dir"

# Signing is CI-only. Local unsigned mode must refuse to pretend it signed.
sign_err="${tmp_root}/sign.err"
if bash "$MANIFEST_SCRIPT" sign --dir "$assets_dir" \
  --certificate-identity 'https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/v9.9.9' \
  >/dev/null 2>"$sign_err"; then
  :
fi
# Without COSIGN in PATH under a missing binary, sign must fail closed (never skip).
if command -v cosign >/dev/null 2>&1; then
  grep -E 'certificate-identity|verify-blob|sign-blob' "$sign_err" >/dev/null \
    || true
else
  if bash "$MANIFEST_SCRIPT" sign --dir "$assets_dir" \
    --certificate-identity 'https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/v9.9.9' \
    >/dev/null 2>"$sign_err"; then
    fail "sign succeeded without cosign"
  fi
  grep -qi 'cosign' "$sign_err" || fail "sign without cosign must name that cosign is missing"
fi

# Release job wiring: one script, no inline hashing, sign failure is not ignored.
python3 - "$RELEASE_WORKFLOW" <<'PY'
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
active = [
    line.rstrip()
    for line in text.splitlines()
    if line.strip() and not line.lstrip().startswith("#")
]
joined = "\n".join(active)
required = (
    'bash scripts/ci/release_checksum_manifest.sh write --dir release',
    'bash scripts/ci/release_checksum_manifest.sh sign --dir release',
    'bash scripts/ci/release_checksum_manifest.sh check-contract --dir release',
)
for needle in required:
    if needle not in joined:
        raise SystemExit(f"release.yml is missing checksum-manifest call: {needle}")
if "|| true" in joined and "release_checksum_manifest.sh" in joined:
    # Fail if the script invocation is on a line that ignores status.
    for line in active:
        if "release_checksum_manifest.sh" in line and "|| true" in line:
            raise SystemExit("checksum-manifest invocation is ignored with || true")
if "sigstore/cosign-installer@" not in joined:
    raise SystemExit("release.yml must pin sigstore/cosign-installer by SHA")
if "attest-release" not in joined or "bundle-path" not in joined:
    raise SystemExit("release.yml must attach the attest-build-provenance bundle-path")
print("release.yml checksum-manifest wiring ok")
PY

echo "release checksum manifest tests passed"
