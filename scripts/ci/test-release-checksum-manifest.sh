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

# Fake-cosign version gate for scripts/install.sh (GHSA-fx35-mq7g-6g98).
# Old / unparsable binaries must refuse verify-blob. A fixed binary may verify.
INSTALLER="${REPO_ROOT}/scripts/install.sh"
[[ -f "$INSTALLER" ]] || fail "scripts/install.sh is missing"

make_versioned_cosign() {
  local path="$1"
  local mode="$2"
  local log="${path}.log"
  : >"$log"
  cat >"$path" <<EOF
#!/bin/sh
set -eu
{
  printf '%s\\n' '--- invocation ---'
  printf '%s\\n' "\$@"
} >> "$log"
case "\$1" in
  version)
    case "$mode" in
      old) printf '%s\\n' 'GitVersion:    v3.0.6' ;;
      fixed) printf '%s\\n' 'GitVersion:    v3.1.3' ;;
      unparsable) printf '%s\\n' 'cosign (devel)' ;;
      *)
        echo "test bug: unknown cosign mode $mode" >&2
        exit 2
        ;;
    esac
    exit 0
    ;;
  verify-blob)
    echo 'Verified OK'
    exit 0
    ;;
  *)
    echo "unexpected cosign invocation: \$*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "$path"
}

run_installer_with_cosign() {
  local case_dir="$1"
  local mode="$2"
  local target="x86_64-unknown-linux-gnu"
  local version="v5.5.2"
  local archive_name="assay-${version}-${target}.tar.gz"
  mkdir -p "$case_dir/bin" "$case_dir/home" "$case_dir/install" "$case_dir/tmp" \
    "$case_dir/payload/assay-${version}-${target}"
  printf '%s\n' 'assay 5.5.2 fixture' >"$case_dir/payload/assay-${version}-${target}/assay"
  chmod +x "$case_dir/payload/assay-${version}-${target}/assay"
  tar -C "$case_dir/payload" -czf "$case_dir/$archive_name" "assay-${version}-${target}"
  printf '%s  %s\n' "$(compute_sha256 "$case_dir/$archive_name")" "$archive_name" \
    >"$case_dir/${archive_name}.sha256"
  printf '%s  %s\n' "$(compute_sha256 "$case_dir/$archive_name")" "$archive_name" \
    >"$case_dir/checksums.txt"
  printf 'sigstore-bundle-fixture\n' >"$case_dir/checksums.txt.sigstore.json"
  cat >"$case_dir/bin/uname" <<'EOF'
#!/bin/sh
case "$1" in
  -s) printf '%s\n' Linux ;;
  -m) printf '%s\n' x86_64 ;;
  *) exit 2 ;;
esac
EOF
  chmod +x "$case_dir/bin/uname"
  cat >"$case_dir/bin/curl" <<EOF
#!/bin/sh
set -eu
out=""
wants_status=0
url=""
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o) out="\$2"; shift 2 ;;
    -w) wants_status=1; shift 2 ;;
    --max-filesize) shift 2 ;;
    -*) shift ;;
    *) url="\$1"; shift ;;
  esac
done
case "\$url" in
  *.tar.gz.sha256) cp "$case_dir/${archive_name}.sha256" "\$out" ;;
  *.tar.gz) cp "$case_dir/$archive_name" "\$out" ;;
  */checksums.txt) cp "$case_dir/checksums.txt" "\$out" ;;
  */checksums.txt.sigstore.json) cp "$case_dir/checksums.txt.sigstore.json" "\$out" ;;
  *) echo "unexpected curl URL: \$url" >&2; exit 2 ;;
esac
if [ "\$wants_status" -eq 1 ]; then
  printf '%s' 200
fi
EOF
  chmod +x "$case_dir/bin/curl"
  make_versioned_cosign "$case_dir/bin/cosign" "$mode"
  env \
    HOME="$case_dir/home" \
    PATH="$case_dir/bin:/usr/bin:/bin" \
    ASSAY_VERSION=5.5.2 \
    ASSAY_INSTALL_DIR="$case_dir/install" \
    TMPDIR="$case_dir/tmp" \
    ASSAY_COSIGN="$case_dir/bin/cosign" \
    sh "$INSTALLER"
}

assert_cosign_version_refused() {
  local mode="$1"
  local case_dir="${tmp_root}/cosign-${mode}"
  mkdir -p "$case_dir"
  if run_installer_with_cosign "$case_dir" "$mode" \
    >"$case_dir/stdout" 2>"$case_dir/stderr"; then
    fail "cosign ${mode} unexpectedly installed"
  fi
  if ! grep -Fq 'GHSA-fx35-mq7g-6g98' "$case_dir/stdout" "$case_dir/stderr"; then
    cat "$case_dir/stdout" "$case_dir/stderr" >&2
    fail "cosign ${mode} refusal must name GHSA-fx35-mq7g-6g98"
  fi
  if ! grep -Eq 'v3\.1\.3|2\.6\.5' "$case_dir/stdout" "$case_dir/stderr"; then
    cat "$case_dir/stdout" "$case_dir/stderr" >&2
    fail "cosign ${mode} refusal must name the fixed minimum versions"
  fi
  if grep -Fq 'verify-blob' "$case_dir/bin/cosign.log"; then
    fail "cosign ${mode} must not reach verify-blob"
  fi
  if grep -Fq 'signed_manifest_verified' "$case_dir/stdout"; then
    fail "cosign ${mode} claimed signed_manifest_verified"
  fi
}

assert_cosign_version_verified() {
  local case_dir="${tmp_root}/cosign-fixed"
  mkdir -p "$case_dir"
  run_installer_with_cosign "$case_dir" fixed \
    >"$case_dir/stdout" 2>"$case_dir/stderr" \
    || {
      cat "$case_dir/stdout" "$case_dir/stderr" >&2
      fail "fixed cosign unexpectedly refused install"
    }
  grep -Fq 'signed_manifest_verified' "$case_dir/stdout" \
    || fail "fixed cosign did not report signed_manifest_verified"
  grep -Fq 'verify-blob' "$case_dir/bin/cosign.log" \
    || fail "fixed cosign did not run verify-blob"
}

assert_cosign_version_refused old
assert_cosign_version_verified
assert_cosign_version_refused unparsable

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
if "cosign-release: v3.1.3" not in joined:
    raise SystemExit(
        "release.yml must pin cosign-release to v3.1.3 (GHSA-fx35-mq7g-6g98)"
    )
if "attest-release" not in joined or "bundle-path" not in joined:
    raise SystemExit("release.yml must attach the attest-build-provenance bundle-path")
print("release.yml checksum-manifest wiring ok")
PY

# User-facing signed-manifest recipe: one selected archive, inclusion enforced.
# The producer lists every payload; a clean-directory user has downloaded one.
extract_signed_manifest_recipe() {
  python3 - "$1" <<'PY'
from pathlib import Path
import re
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
fences = re.findall(r"```bash\n(.*?)```", text, re.S)
chosen = [
    fence
    for fence in fences
    if "cosign verify-blob" in fence and "checksums.txt" in fence
]
if len(chosen) != 1:
    raise SystemExit(
        f"{sys.argv[1]}: expected 1 signed-manifest bash fence, got {len(chosen)}"
    )
sys.stdout.write(chosen[0])
PY
}

INSTALL_DOC="${REPO_ROOT}/docs/getting-started/installation.md"
RELEASE_DOC="${REPO_ROOT}/docs/reference/release.md"
install_recipe="$(extract_signed_manifest_recipe "$INSTALL_DOC")"
release_recipe="$(extract_signed_manifest_recipe "$RELEASE_DOC")"
[[ -n "$install_recipe" ]] || fail "installation.md signed-manifest fence was empty"
[[ -n "$release_recipe" ]] || fail "release.md signed-manifest fence was empty"

recipe_doc_dir="${tmp_root}/doc-recipes"
mkdir -p "$recipe_doc_dir"
printf '%s' "$install_recipe" >"${recipe_doc_dir}/installation.sh"
printf '%s' "$release_recipe" >"${recipe_doc_dir}/release.sh"

selected_name='assay-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz'
extra_name='assay-vX.Y.Z-sbom-cyclonedx.tar.gz'

prepare_recipe_store() {
  local store="$1"
  mkdir -p "$store"
  printf 'selected-archive\n' >"${store}/${selected_name}"
  printf 'other-payload\n' >"${store}/${extra_name}"
  {
    printf '%s  %s\n' "$(compute_sha256 "${store}/${selected_name}")" "$selected_name"
    printf '%s  %s\n' "$(compute_sha256 "${store}/${extra_name}")" "$extra_name"
  } | LC_ALL=C sort >"${store}/checksums.txt"
  printf 'sigstore-bundle-fixture\n' >"${store}/checksums.txt.sigstore.json"
}

run_extracted_recipe() {
  local recipe_file="$1"
  local case_dir="$2"
  local store="$3"
  mkdir -p "${case_dir}/bin" "${case_dir}/run"
  : >"${case_dir}/curl.log"
  : >"${case_dir}/cosign.log"
  cat >"${case_dir}/bin/curl" <<EOF
#!/bin/sh
set -eu
printf '%s\\n' "\$*" >> "${case_dir}/curl.log"
url=""
out=""
use_remote_name=0
while [ "\$#" -gt 0 ]; do
  case "\$1" in
    -o)
      out="\$2"
      shift 2
      ;;
    -O)
      use_remote_name=1
      shift
      ;;
    -fsSLO|-fsSL|-sSLO|-sSL|-LO)
      case "\$1" in
        *O*) use_remote_name=1 ;;
      esac
      shift
      ;;
    -*)
      shift
      ;;
    *)
      url="\$1"
      shift
      ;;
  esac
done
[ -n "\$url" ] || { echo "curl: missing URL" >&2; exit 2; }
base=\$(basename "\$url")
if [ -z "\$out" ] && [ "\$use_remote_name" -eq 1 ]; then
  out="\$base"
fi
[ -n "\$out" ] || { echo "curl: missing output path" >&2; exit 2; }
if [ ! -f "${store}/\$base" ]; then
  echo "curl: fixture not found: \$base" >&2
  exit 22
fi
cp "${store}/\$base" "\$out"
EOF
  chmod +x "${case_dir}/bin/curl"
  cat >"${case_dir}/bin/cosign" <<EOF
#!/bin/sh
set -eu
printf '%s\\n' "\$*" >> "${case_dir}/cosign.log"
echo 'Verified OK'
EOF
  chmod +x "${case_dir}/bin/cosign"
  sha256sum_dir="$(dirname "$(command -v sha256sum)")"
  [[ -n "$sha256sum_dir" && -x "${sha256sum_dir}/sha256sum" ]] \
    || fail "sha256sum is required to execute the documented recipe"
  (
    cd "${case_dir}/run"
    env PATH="${case_dir}/bin:${sha256sum_dir}:/usr/bin:/bin" /bin/sh "$recipe_file"
  )
}

store="${tmp_root}/doc-recipe-store"
prepare_recipe_store "$store"

# RED/GREEN: the literal installation recipe must succeed when only the
# selected archive is present beside a full multi-payload manifest.
happy_dir="${tmp_root}/doc-recipe-happy"
mkdir -p "$happy_dir"
if ! run_extracted_recipe "${recipe_doc_dir}/installation.sh" "$happy_dir" "$store" \
  >"${happy_dir}/stdout" 2>"${happy_dir}/stderr"; then
  cat "${happy_dir}/stdout" "${happy_dir}/stderr" >&2
  fail "literal installation.md recipe failed in a one-archive directory"
fi
grep -Fq 'Verified OK' "${happy_dir}/stdout" \
  || fail "installation.md recipe did not print Verified OK before hashes"
grep -Fq "${selected_name}: OK" "${happy_dir}/stdout" \
  || fail "installation.md recipe did not print OK for the selected archive"
if grep -Fq "$extra_name" "${happy_dir}/curl.log"; then
  fail "installation.md recipe downloaded an unselected payload"
fi
if grep -Fq -- '--ignore-missing' "${recipe_doc_dir}/installation.sh" \
  "${recipe_doc_dir}/release.sh"; then
  fail "signed-manifest recipe uses --ignore-missing"
fi

python3 - "${recipe_doc_dir}/installation.sh" "${recipe_doc_dir}/release.sh" <<'PY'
from pathlib import Path
import sys

for path in sys.argv[1:]:
    text = Path(path).read_text(encoding="utf-8")
    verify_at = text.find("cosign verify-blob")
    hash_at = text.find("sha256sum")
    if verify_at < 0 or hash_at < 0 or verify_at > hash_at:
        raise SystemExit(f"{path}: signature check must precede sha256sum")
    if "--ignore-missing" in text:
        raise SystemExit(f"{path}: --ignore-missing is forbidden")
print("signed-manifest recipe order ok")
PY

# release.md must be the same selected-archive recipe, then the same run.
if [[ "$install_recipe" != "$release_recipe" ]]; then
  fail "installation.md and release.md signed-manifest recipes must match"
fi
release_happy="${tmp_root}/doc-recipe-release-happy"
mkdir -p "$release_happy"
run_extracted_recipe "${recipe_doc_dir}/release.sh" "$release_happy" "$store" \
  >"${release_happy}/stdout" 2>"${release_happy}/stderr" \
  || fail "literal release.md recipe failed in a one-archive directory"

# Negative: missing selected archive must not pass.
missing_store="${tmp_root}/doc-recipe-missing-store"
prepare_recipe_store "$missing_store"
rm -f "${missing_store}/${selected_name}"
missing_dir="${tmp_root}/doc-recipe-missing"
mkdir -p "$missing_dir"
if run_extracted_recipe "${recipe_doc_dir}/installation.sh" "$missing_dir" \
  "$missing_store" >"${missing_dir}/stdout" 2>"${missing_dir}/stderr"; then
  fail "missing selected archive was accepted by the documented recipe"
fi

# Negative: selected archive absent from checksums.txt must not pass.
omit_store="${tmp_root}/doc-recipe-omit-store"
prepare_recipe_store "$omit_store"
grep -Fv "$selected_name" "${omit_store}/checksums.txt" >"${omit_store}/checksums.omit"
mv "${omit_store}/checksums.omit" "${omit_store}/checksums.txt"
omit_dir="${tmp_root}/doc-recipe-omit"
mkdir -p "$omit_dir"
if run_extracted_recipe "${recipe_doc_dir}/installation.sh" "$omit_dir" \
  "$omit_store" >"${omit_dir}/stdout" 2>"${omit_dir}/stderr"; then
  fail "absent manifest entry was accepted by the documented recipe"
fi
if ! grep -Fq "$selected_name" "${omit_dir}/stdout" "${omit_dir}/stderr"; then
  cat "${omit_dir}/stdout" "${omit_dir}/stderr" >&2
  fail "absent manifest entry must name the selected archive"
fi

# Negative: tampered selected bytes must not pass.
tamper_store="${tmp_root}/doc-recipe-tamper-store"
prepare_recipe_store "$tamper_store"
printf 'tampered-archive\n' >"${tamper_store}/${selected_name}"
tamper_dir="${tmp_root}/doc-recipe-tamper"
mkdir -p "$tamper_dir"
if run_extracted_recipe "${recipe_doc_dir}/installation.sh" "$tamper_dir" \
  "$tamper_store" >"${tamper_dir}/stdout" 2>"${tamper_dir}/stderr"; then
  fail "tampered selected archive was accepted by the documented recipe"
fi
if ! grep -Eq 'FAILED|did NOT match' "${tamper_dir}/stdout" "${tamper_dir}/stderr"; then
  cat "${tamper_dir}/stdout" "${tamper_dir}/stderr" >&2
  fail "tampered archive must fail the hash check"
fi

echo "release checksum manifest tests passed"
