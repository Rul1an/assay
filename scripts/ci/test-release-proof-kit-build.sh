#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/ci/release_proof_kit_build.sh"
TEST_TEMP_DIR=""

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

make_fake_gh() {
  local path="$1"
  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$FAKE_GH_LOG"

if [ "${1:-}" = "--version" ] || [ "${1:-}" = "version" ]; then
  echo "gh version 2.88.1 (2026-03-12)"
  exit 0
fi

if [ "${1:-}" = "attestation" ] && [ "${2:-}" = "trusted-root" ]; then
  if [ "${FAKE_TRUSTED_ROOT_FAIL:-0}" = "1" ]; then
    echo "simulated trusted-root failure" >&2
    exit 1
  fi
  cat "$FAKE_TRUSTED_ROOT_CONTENT"
  exit 0
fi

if [ "${1:-}" = "attestation" ] && [ "${2:-}" = "download" ]; then
  mode="${FAKE_DOWNLOAD_MODE:-single}"
  case "$mode" in
    single)
      cp "$FAKE_DOWNLOAD_CONTENT" "$PWD/$FAKE_DOWNLOAD_FILENAME"
      ;;
    none)
      ;;
    multiple)
      cp "$FAKE_DOWNLOAD_CONTENT" "$PWD/$FAKE_DOWNLOAD_FILENAME"
      cp "$FAKE_DOWNLOAD_CONTENT" "$PWD/${FAKE_DOWNLOAD_FILENAME}.extra"
      ;;
    fail)
      echo "simulated download failure" >&2
      exit 1
      ;;
    *)
      echo "unknown download mode: $mode" >&2
      exit 1
      ;;
  esac
  exit 0
fi

if [ "${1:-}" = "attestation" ] && [ "${2:-}" = "verify" ]; then
  exit 0
fi

if [ "${1:-}" = "release" ] && [ "${2:-}" = "verify" ]; then
  exit 0
fi

if [ "${1:-}" = "release" ] && [ "${2:-}" = "verify-asset" ]; then
  exit 0
fi

echo "unexpected gh invocation: $*" >&2
exit 1
EOF
  chmod +x "$path"
}

write_summary() {
  local path="$1"
  local asset_name="$2"
  local asset_digest="$3"
  cat >"$path" <<EOF
{
  "schema_version": 1,
  "verification_policy": {
    "repo": "Rul1an/assay",
    "signer_workflow": "Rul1an/assay/.github/workflows/release.yml",
    "cert_oidc_issuer": "https://token.actions.githubusercontent.com",
    "source_ref": "refs/tags/v9.9.9",
    "source_digest": "abc123def456",
    "predicate_type": "https://slsa.dev/provenance/v1",
    "deny_self_hosted_runners": true
  },
  "assets": [
    {
      "name": "$asset_name",
      "sha256": "$asset_digest",
      "raw_attestation_file": "$asset_name.attestation.json",
      "verified_attestations": 1,
      "predicate_types": [
        "https://slsa.dev/provenance/v1"
      ],
      "verified_timestamp_count": 1,
      "subjects": []
    }
  ]
}
EOF
}

write_summary_sha() {
  local summary="$1"
  local out="$2"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$summary" >"$out"
  else
    shasum -a 256 "$summary" >"$out"
  fi
}

setup_success_fixture() {
  local temp_dir="$1"
  local assets_dir="$temp_dir/assets"
  local summary="$temp_dir/release-provenance.json"
  local summary_sha="$temp_dir/release-provenance.json.sha256"
  local trusted_root="$temp_dir/trusted_root.jsonl"
  local bundle="$temp_dir/downloaded-bundle.jsonl"
  local fake_gh="$temp_dir/fake-gh"
  local gh_log="$temp_dir/gh.log"
  local asset_name="assay-v9.9.9-x86_64-unknown-linux-gnu.tar.gz"
  local asset_path="$assets_dir/$asset_name"

  mkdir -p "$assets_dir"
  printf 'release-bytes\n' >"$asset_path"
  local digest
  digest="$(compute_sha256 "$asset_path")"

  write_summary "$summary" "$asset_name" "$digest"
  write_summary_sha "$summary" "$summary_sha"

  printf '{"trusted":"root"}\n' >"$trusted_root"
  printf '{"bundle":"ok"}\n' >"$bundle"
  : >"$gh_log"
  make_fake_gh "$fake_gh"

  SUCCESS_ASSETS_DIR="$assets_dir"
  SUCCESS_SUMMARY="$summary"
  SUCCESS_SUMMARY_SHA="$summary_sha"
  SUCCESS_TRUSTED_ROOT="$trusted_root"
  SUCCESS_BUNDLE="$bundle"
  SUCCESS_FAKE_GH="$fake_gh"
  SUCCESS_GH_LOG="$gh_log"
  SUCCESS_ASSET_NAME="$asset_name"
}

run_success_case() {
  local temp_dir="$1"
  setup_success_fixture "$temp_dir"
  local out_archive="$temp_dir/release/assay-v9.9.9-release-proof-kit.tar.gz"

  FAKE_GH_LOG="$SUCCESS_GH_LOG" \
  FAKE_TRUSTED_ROOT_CONTENT="$SUCCESS_TRUSTED_ROOT" \
  FAKE_DOWNLOAD_CONTENT="$SUCCESS_BUNDLE" \
  FAKE_DOWNLOAD_FILENAME="sha256:1234.jsonl" \
  FAKE_DOWNLOAD_MODE="single" \
  GH_BIN="$SUCCESS_FAKE_GH" \
  ASSETS_DIR="$SUCCESS_ASSETS_DIR" \
  PROVENANCE_SUMMARY="$SUCCESS_SUMMARY" \
  PROVENANCE_SUMMARY_SHA256="$SUCCESS_SUMMARY_SHA" \
  OUT_ARCHIVE="$out_archive" \
  VERSION="v9.9.9" \
  bash "$SCRIPT"

  test -f "$out_archive"

  local extract_dir="$temp_dir/extract"
  mkdir -p "$extract_dir"
  tar -xzf "$out_archive" -C "$extract_dir"

  local kit_root="$extract_dir/release-proof-kit"
  test -f "$kit_root/manifest.json"
  test -f "$kit_root/trusted_root.jsonl"
  test -f "$kit_root/release-provenance.json"
  test -f "$kit_root/release-provenance.json.sha256"
  test -f "$kit_root/bundles/$SUCCESS_ASSET_NAME.jsonl"
  test -f "$kit_root/verify-offline.sh"
  test -f "$kit_root/verify-release-online.sh"
  test -f "$kit_root/README.md"

  jq -e --slurpfile summary "$SUCCESS_SUMMARY" '
    .repo == $summary[0].verification_policy.repo and
    .signer_workflow == $summary[0].verification_policy.signer_workflow and
    .cert_oidc_issuer == $summary[0].verification_policy.cert_oidc_issuer and
    .source_ref == $summary[0].verification_policy.source_ref and
    .source_digest == $summary[0].verification_policy.source_digest and
    .predicate_type == $summary[0].verification_policy.predicate_type and
    .deny_self_hosted_runners == $summary[0].verification_policy.deny_self_hosted_runners
  ' "$kit_root/manifest.json" >/dev/null

  grep -F 'canonical offline verification path' "$kit_root/README.md" >/dev/null
  grep -F 'general Sigstore verification' "$kit_root/README.md" >/dev/null
  grep -F 'convenience-only' "$kit_root/verify-release-online.sh" >/dev/null

  local verify_log="$temp_dir/verify-offline.log"
  : >"$verify_log"
  FAKE_GH_LOG="$verify_log" \
  GH_BIN="$SUCCESS_FAKE_GH" \
  JQ_BIN="jq" \
  "$kit_root/verify-offline.sh" --assets-dir "$SUCCESS_ASSETS_DIR"

  grep -F 'attestation verify' "$verify_log" >/dev/null
  grep -F -- '--bundle' "$verify_log" >/dev/null
  grep -F -- '--custom-trusted-root' "$verify_log" >/dev/null
  grep -F -- '--repo Rul1an/assay' "$verify_log" >/dev/null
  grep -F -- '--signer-workflow Rul1an/assay/.github/workflows/release.yml' "$verify_log" >/dev/null
  grep -F -- '--source-ref refs/tags/v9.9.9' "$verify_log" >/dev/null
  grep -F -- '--source-digest abc123def456' "$verify_log" >/dev/null
  grep -F -- '--predicate-type https://slsa.dev/provenance/v1' "$verify_log" >/dev/null
  grep -F -- '--deny-self-hosted-runners' "$verify_log" >/dev/null
  grep -F "attestation download ${SUCCESS_ASSETS_DIR}/${SUCCESS_ASSET_NAME}" "$SUCCESS_GH_LOG" >/dev/null
}

run_asset_set_mismatch_case() {
  local temp_dir="$1"
  setup_success_fixture "$temp_dir"
  printf 'extra\n' >"$SUCCESS_ASSETS_DIR/extra.zip"
  local out_archive="$temp_dir/release/mismatch.tar.gz"

  if FAKE_GH_LOG="$SUCCESS_GH_LOG" \
    FAKE_TRUSTED_ROOT_CONTENT="$SUCCESS_TRUSTED_ROOT" \
    FAKE_DOWNLOAD_CONTENT="$SUCCESS_BUNDLE" \
    FAKE_DOWNLOAD_FILENAME="sha256:1234.jsonl" \
    FAKE_DOWNLOAD_MODE="single" \
    GH_BIN="$SUCCESS_FAKE_GH" \
    ASSETS_DIR="$SUCCESS_ASSETS_DIR" \
    PROVENANCE_SUMMARY="$SUCCESS_SUMMARY" \
    PROVENANCE_SUMMARY_SHA256="$SUCCESS_SUMMARY_SHA" \
    OUT_ARCHIVE="$out_archive" \
    VERSION="v9.9.9" \
    bash "$SCRIPT" >/dev/null 2>&1; then
    echo "expected asset-set mismatch to fail" >&2
    exit 1
  fi

  test ! -e "$out_archive"
}

run_missing_bundle_case() {
  local temp_dir="$1"
  setup_success_fixture "$temp_dir"
  local out_archive="$temp_dir/release/missing-bundle.tar.gz"

  if FAKE_GH_LOG="$SUCCESS_GH_LOG" \
    FAKE_TRUSTED_ROOT_CONTENT="$SUCCESS_TRUSTED_ROOT" \
    FAKE_DOWNLOAD_CONTENT="$SUCCESS_BUNDLE" \
    FAKE_DOWNLOAD_FILENAME="sha256:1234.jsonl" \
    FAKE_DOWNLOAD_MODE="none" \
    GH_BIN="$SUCCESS_FAKE_GH" \
    ASSETS_DIR="$SUCCESS_ASSETS_DIR" \
    PROVENANCE_SUMMARY="$SUCCESS_SUMMARY" \
    PROVENANCE_SUMMARY_SHA256="$SUCCESS_SUMMARY_SHA" \
    OUT_ARCHIVE="$out_archive" \
    VERSION="v9.9.9" \
    bash "$SCRIPT" >/dev/null 2>&1; then
    echo "expected missing bundle case to fail" >&2
    exit 1
  fi

  test ! -e "$out_archive"
}

run_trusted_root_failure_case() {
  local temp_dir="$1"
  setup_success_fixture "$temp_dir"
  local out_archive="$temp_dir/release/trusted-root-failure.tar.gz"

  if FAKE_GH_LOG="$SUCCESS_GH_LOG" \
    FAKE_TRUSTED_ROOT_CONTENT="$SUCCESS_TRUSTED_ROOT" \
    FAKE_TRUSTED_ROOT_FAIL="1" \
    FAKE_DOWNLOAD_CONTENT="$SUCCESS_BUNDLE" \
    FAKE_DOWNLOAD_FILENAME="sha256:1234.jsonl" \
    FAKE_DOWNLOAD_MODE="single" \
    GH_BIN="$SUCCESS_FAKE_GH" \
    ASSETS_DIR="$SUCCESS_ASSETS_DIR" \
    PROVENANCE_SUMMARY="$SUCCESS_SUMMARY" \
    PROVENANCE_SUMMARY_SHA256="$SUCCESS_SUMMARY_SHA" \
    OUT_ARCHIVE="$out_archive" \
    VERSION="v9.9.9" \
    bash "$SCRIPT" >/dev/null 2>&1; then
    echo "expected trusted-root failure case to fail" >&2
    exit 1
  fi

  test ! -e "$out_archive"
}

cleanup() {
  if [ -n "$TEST_TEMP_DIR" ]; then
    rm -rf "$TEST_TEMP_DIR"
  fi
}

main() {
  TEST_TEMP_DIR="$(mktemp -d)"
  trap cleanup EXIT

  run_success_case "$TEST_TEMP_DIR"
  run_asset_set_mismatch_case "$TEST_TEMP_DIR"
  run_missing_bundle_case "$TEST_TEMP_DIR"
  run_trusted_root_failure_case "$TEST_TEMP_DIR"

  echo "release proof kit build contract tests: PASS"
}

main "$@"
