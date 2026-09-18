#!/usr/bin/env bash
# Download one published CLI archive by tag, verify its sidecar and attestation,
# extract it the way scripts/install.sh extracts a GitHub release asset, then
# run the #3104 golden-path opening against that binary.
#
# The archive URL is always https://github.com/<repo>/releases/download/<tag>/...
# A same-run build artifact cannot satisfy this script.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPO="${GITHUB_REPOSITORY:-Rul1an/assay}"
GH_BIN="${GH_BIN:-gh}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

usage() {
  echo "usage: published-release-platform-opening.sh --release-tag vX.Y.Z --target <triple> --run-root <abs-path>" >&2
  exit 2
}

release_tag=""
target=""
run_root=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --release-tag)
      [[ "$#" -ge 2 ]] || usage
      release_tag="$2"
      shift 2
      ;;
    --target)
      [[ "$#" -ge 2 ]] || usage
      target="$2"
      shift 2
      ;;
    --run-root)
      [[ "$#" -ge 2 ]] || usage
      run_root="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release tag must be an exact stable vX.Y.Z tag"
if [[ "$run_root" != /* ]] && command -v cygpath >/dev/null 2>&1; then
  run_root="$(cygpath -u "$run_root")"
fi
[[ "$run_root" = /* ]] || fail "run root must be absolute"
[[ ! -e "$run_root" ]] || fail "run root already exists; refusing to reuse prior evidence: $run_root"

case "$target" in
  x86_64-pc-windows-msvc)
    archive_ext="zip"
    binary_name="assay.exe"
    ;;
  aarch64-apple-darwin)
    archive_ext="tar.gz"
    binary_name="assay"
    ;;
  *)
    fail "opening driver supports only x86_64-pc-windows-msvc or aarch64-apple-darwin"
    ;;
esac

for required in "$GH_BIN" "$PYTHON_BIN"; do
  command -v "$required" >/dev/null 2>&1 || fail "missing required command: $required"
done

expected_version="${release_tag#v}"
asset_name="assay-${release_tag}-${target}.${archive_ext}"
sidecar_name="${asset_name}.sha256"
asset_url="https://github.com/${REPO}/releases/download/${release_tag}/${asset_name}"
sidecar_url="https://github.com/${REPO}/releases/download/${release_tag}/${sidecar_name}"

downloads="$run_root/downloads"
extract="$run_root/extract"
results="$run_root/results"
mkdir -p "$downloads" "$extract" "$results"

printf '%s\n' "$asset_url" >"$results/download-url.txt"
printf '%s\n' "$sidecar_url" >"$results/sidecar-url.txt"
printf '%s\n' "$release_tag" >"$results/release-tag.txt"
printf '%s\n' "$target" >"$results/target.txt"

sidecar_max_bytes=$((64 + 2 + ${#asset_name} + 1))
PYTHONPATH="$ROOT/scripts/ci" "$PYTHON_BIN" -c \
  'import pathlib,sys; from bounded_download import download; download(sys.argv[1], pathlib.Path(sys.argv[2]), max_bytes=int(sys.argv[3]))' \
  "$asset_url" "$downloads/$asset_name" 67108864
PYTHONPATH="$ROOT/scripts/ci" "$PYTHON_BIN" -c \
  'import pathlib,sys; from bounded_download import download; download(sys.argv[1], pathlib.Path(sys.argv[2]), max_bytes=int(sys.argv[3]))' \
  "$sidecar_url" "$downloads/$sidecar_name" "$sidecar_max_bytes"

"$PYTHON_BIN" - "$downloads/$asset_name" "$downloads/$sidecar_name" "$asset_name" <<'PY'
import hashlib, pathlib, sys

archive, sidecar, asset_name = sys.argv[1], pathlib.Path(sys.argv[2]), sys.argv[3]
data = sidecar.read_bytes()
expected_record = (64 + 2 + len(asset_name.encode()) + 1)
if len(data) != expected_record or data.count(b"\n") != 1 or data.endswith(b"\r\n"):
    raise SystemExit("Checksum sidecar must contain exactly one newline-terminated record.")
line = data.decode("ascii").rstrip("\n")
suffix = f"  {asset_name}"
if not line.endswith(suffix):
    raise SystemExit("Checksum sidecar does not name the selected archive.")
expected = line[: -len(suffix)]
if len(expected) != 64 or any(ch not in "0123456789abcdef" for ch in expected):
    raise SystemExit("Checksum sidecar SHA-256 must be lowercase hexadecimal.")
actual = hashlib.sha256(pathlib.Path(archive).read_bytes()).hexdigest()
if actual != expected:
    raise SystemExit(f"archive checksum mismatch for {asset_name}")
print(f"verification=checksum_verified asset={asset_name} sha256={actual}")
PY

tag_ref="$results/tag-ref.json"
"$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}" >"$tag_ref"
source_type="$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["object"]["type"])' "$tag_ref")"
source_digest="$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["object"]["sha"])' "$tag_ref")"
for _ in 1 2 3 4; do
  [[ "$source_type" != "tag" ]] || {
    tag_object="$results/tag-object.json"
    "$GH_BIN" api "repos/${REPO}/git/tags/${source_digest}" >"$tag_object"
    source_type="$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["object"]["type"])' "$tag_object")"
    source_digest="$("$PYTHON_BIN" -c 'import json,sys; print(json.load(open(sys.argv[1], encoding="utf-8"))["object"]["sha"])' "$tag_object")"
  }
done
[[ "$source_type" == "commit" && "$source_digest" =~ ^[0-9a-f]{40}$ ]] \
  || fail "release tag does not resolve to a commit within four tag objects"

if ! gh attestation verify "$downloads/$asset_name" \
  --repo "$REPO" \
  --signer-workflow "$REPO/.github/workflows/release.yml" \
  --cert-oidc-issuer "https://token.actions.githubusercontent.com" \
  --predicate-type "https://slsa.dev/provenance/v1" \
  --source-digest "$source_digest" \
  --deny-self-hosted-runners \
  >"$results/attestation-verify.log" 2>&1; then
  cat "$results/attestation-verify.log" >&2
  fail "gh attestation verify rejected the published archive"
fi

unset GH_TOKEN GITHUB_TOKEN

(
  cd "$extract"
  if [[ "$archive_ext" == "zip" ]]; then
    command -v unzip >/dev/null 2>&1 || fail "unzip is required for Windows installation."
    unzip -q "$downloads/$asset_name"
  else
    tar xzkf "$downloads/$asset_name"
  fi
)

extracted_dir="$extract/assay-${release_tag}-${target}"
if [[ -f "$extracted_dir/$binary_name" ]]; then
  assay_bin="$extracted_dir/$binary_name"
elif [[ -f "$extract/$binary_name" ]]; then
  assay_bin="$extract/$binary_name"
else
  fail "Could not find $binary_name after extraction"
fi
assay_bin="$("$PYTHON_BIN" -c 'import os,sys; print(os.path.abspath(sys.argv[1]))' "$assay_bin")"
[[ -f "$assay_bin" ]] || fail "extracted binary is not a file: $assay_bin"

version_out="$("$assay_bin" version)"
if [[ -z "$version_out" ]]; then
  echo "::error::assay version produced empty stdout" >&2
  fail "assay version produced empty stdout"
fi
if [[ "$version_out" != "$expected_version" ]]; then
  echo "::error::assay version mismatch: expected ${expected_version}, got ${version_out}" >&2
  fail "assay version mismatch: expected ${expected_version}, got ${version_out}"
fi
printf '%s\n' "$version_out" >"$results/version.txt"

doctor_out="$("$assay_bin" doctor --format json)"
if [[ -z "$doctor_out" ]]; then
  echo "::error::assay doctor --format json produced empty stdout" >&2
  fail "assay doctor --format json produced empty stdout"
fi
printf '%s\n' "$doctor_out" | "$PYTHON_BIN" -c 'import json, sys; json.load(sys.stdin)'
printf '%s\n' "$doctor_out" >"$results/doctor.json"

init_scratch="$run_root/init-scratch"
mkdir -p "$init_scratch"
init_out="$(
  cd "$init_scratch"
  "$assay_bin" init --preset dev --hello-trace
)"
if [[ -z "$init_out" ]]; then
  echo "::error::assay init produced empty stdout" >&2
  fail "assay init produced empty stdout"
fi
if [[ ! -f "$init_scratch/eval.yaml" || ! -f "$init_scratch/traces/hello.jsonl" ]]; then
  echo "::error::assay init did not scaffold expected files" >&2
  fail "assay init did not scaffold expected files"
fi
printf '%s\n' "$init_out" >"$results/init.stdout"

echo "ok: published CLI opening tag=$release_tag target=$target version=$expected_version"
