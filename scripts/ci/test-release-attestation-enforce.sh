#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/ci/release_attestation_enforce.sh"
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
  cat > "$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >> "$FAKE_GH_LOG"

if [ "$#" -lt 3 ] || [ "$1" != "attestation" ] || [ "$2" != "verify" ]; then
  echo "unexpected gh invocation: $*" >&2
  exit 1
fi

if [ -n "${FAKE_GH_COUNTDOWN_FILE:-}" ] && [ -f "$FAKE_GH_COUNTDOWN_FILE" ]; then
  current_count="$(cat "$FAKE_GH_COUNTDOWN_FILE")"
  if [ "$current_count" -gt 0 ]; then
    next_count=$((current_count - 1))
    printf '%s\n' "$next_count" > "$FAKE_GH_COUNTDOWN_FILE"
    echo "simulated gh failure" >&2
    exit 1
  fi
fi

cat "$FAKE_GH_JSON"
EOF
  chmod +x "$path"
}

write_success_json() {
  local path="$1"
  local digest="$2"
  cat > "$path" <<EOF
[
  {
    "verificationResult": {
      "verifiedTimestamps": [
        {
          "type": "transparency_log",
          "time": "2026-03-21T10:00:00Z"
        }
      ],
      "statement": {
        "predicateType": "https://slsa.dev/provenance/v1",
        "subject": [
          {
            "name": "release/test-asset.tar.gz",
            "digest": {
              "sha256": "$digest"
            }
          }
        ]
      }
    }
  }
]
EOF
}

write_missing_witness_json() {
  local path="$1"
  local digest="$2"
  cat > "$path" <<EOF
[
  {
    "verificationResult": {
      "verifiedTimestamps": [],
      "statement": {
        "predicateType": "https://slsa.dev/provenance/v1",
        "subject": [
          {
            "name": "release/test-asset.tar.gz",
            "digest": {
              "sha256": "$digest"
            }
          }
        ]
      }
    }
  }
]
EOF
}

write_digest_mismatch_json() {
  local path="$1"
  cat > "$path" <<'EOF'
[
  {
    "verificationResult": {
      "verifiedTimestamps": [
        {
          "type": "transparency_log",
          "time": "2026-03-21T10:00:00Z"
        }
      ],
      "statement": {
        "predicateType": "https://slsa.dev/provenance/v1",
        "subject": [
          {
            "name": "release/test-asset.tar.gz",
            "digest": {
              "sha256": "deadbeef"
            }
          }
        ]
      }
    }
  }
]
EOF
}

write_empty_verification_json() {
  local path="$1"
  cat > "$path" <<'EOF'
[]
EOF
}

run_success_case() {
  local temp_dir="$1"
  local assets_dir="$temp_dir/assets-success"
  local raw_dir="$temp_dir/raw-success"
  local summary="$temp_dir/summary-success.json"
  local log="$temp_dir/gh-success.log"
  local json="$temp_dir/gh-success.json"
  local fake_gh="$temp_dir/fake-gh-success"

  mkdir -p "$assets_dir" "$raw_dir"
  printf 'release bytes\n' > "$assets_dir/test-asset.tar.gz"
  local digest
  digest="$(compute_sha256 "$assets_dir/test-asset.tar.gz")"
  write_success_json "$json" "$digest"
  make_fake_gh "$fake_gh"

  FAKE_GH_JSON="$json" \
  FAKE_GH_LOG="$log" \
  GH_BIN="$fake_gh" \
  ASSETS_DIR="$assets_dir" \
  OUT_SUMMARY="$summary" \
  OUT_RAW_DIR="$raw_dir" \
  REPO="Rul1an/assay" \
  SIGNER_WORKFLOW="Rul1an/assay/.github/workflows/release.yml" \
  SOURCE_REF="refs/tags/v9.9.9" \
  SOURCE_DIGEST="abc123" \
  bash "$SCRIPT"

  jq -e '.verification_policy.source_ref == "refs/tags/v9.9.9"' "$summary" >/dev/null
  jq -e --arg digest "$digest" '.assets[0].sha256 == $digest' "$summary" >/dev/null
  jq -e '.assets[0].verified_timestamp_count == 1' "$summary" >/dev/null
  test -f "$raw_dir/test-asset.tar.gz.attestation.json"
  grep -F -- '--source-digest abc123' "$log" >/dev/null
  grep -F -- '--source-ref refs/tags/v9.9.9' "$log" >/dev/null
  grep -F -- '--deny-self-hosted-runners' "$log" >/dev/null
}

run_retry_success_case() {
  local temp_dir="$1"
  local assets_dir="$temp_dir/assets-retry-success"
  local raw_dir="$temp_dir/raw-retry-success"
  local summary="$temp_dir/summary-retry-success.json"
  local stderr_file="$temp_dir/retry-success.stderr"
  local log="$temp_dir/gh-retry-success.log"
  local json="$temp_dir/gh-retry-success.json"
  local countdown_file="$temp_dir/gh-retry-success.countdown"
  local fake_gh="$temp_dir/fake-gh-retry-success"

  mkdir -p "$assets_dir" "$raw_dir"
  printf 'release bytes\n' > "$assets_dir/test-asset.tar.gz"
  local digest
  digest="$(compute_sha256 "$assets_dir/test-asset.tar.gz")"
  write_success_json "$json" "$digest"
  make_fake_gh "$fake_gh"
  printf '1\n' > "$countdown_file"

  FAKE_GH_JSON="$json" \
  FAKE_GH_LOG="$log" \
  FAKE_GH_COUNTDOWN_FILE="$countdown_file" \
  GH_BIN="$fake_gh" \
  ASSETS_DIR="$assets_dir" \
  OUT_SUMMARY="$summary" \
  OUT_RAW_DIR="$raw_dir" \
  REPO="Rul1an/assay" \
  SIGNER_WORKFLOW="Rul1an/assay/.github/workflows/release.yml" \
  SOURCE_REF="refs/tags/v9.9.9" \
  SOURCE_DIGEST="abc123" \
  ATTESTATION_VERIFY_MAX_RETRIES=2 \
  ATTESTATION_VERIFY_RETRY_DELAY_SECONDS=0 \
  bash "$SCRIPT" 2>"$stderr_file"

  grep -F 'retrying in 0s' "$stderr_file" >/dev/null
  jq -e '.assets[0].verified_attestations == 1' "$summary" >/dev/null
}

run_missing_witness_case() {
  local temp_dir="$1"
  local assets_dir="$temp_dir/assets-missing-witness"
  local raw_dir="$temp_dir/raw-missing-witness"
  local summary="$temp_dir/summary-missing-witness.json"
  local stderr_file="$temp_dir/missing-witness.stderr"
  local json="$temp_dir/gh-missing-witness.json"
  local log="$temp_dir/gh-missing-witness.log"
  local fake_gh="$temp_dir/fake-gh-missing-witness"

  mkdir -p "$assets_dir" "$raw_dir"
  printf 'release bytes\n' > "$assets_dir/test-asset.tar.gz"
  local digest
  digest="$(compute_sha256 "$assets_dir/test-asset.tar.gz")"
  write_missing_witness_json "$json" "$digest"
  make_fake_gh "$fake_gh"

  if FAKE_GH_JSON="$json" \
    FAKE_GH_LOG="$log" \
    GH_BIN="$fake_gh" \
    ASSETS_DIR="$assets_dir" \
    OUT_SUMMARY="$summary" \
    OUT_RAW_DIR="$raw_dir" \
    REPO="Rul1an/assay" \
    SIGNER_WORKFLOW="Rul1an/assay/.github/workflows/release.yml" \
    SOURCE_REF="refs/tags/v9.9.9" \
    SOURCE_DIGEST="abc123" \
    bash "$SCRIPT" > /dev/null 2>"$stderr_file"; then
    echo "expected missing witness case to fail" >&2
    exit 1
  fi

  grep -F 'missing transparency/timestamp witnesses' "$stderr_file" >/dev/null
}

run_digest_mismatch_case() {
  local temp_dir="$1"
  local assets_dir="$temp_dir/assets-digest-mismatch"
  local raw_dir="$temp_dir/raw-digest-mismatch"
  local summary="$temp_dir/summary-digest-mismatch.json"
  local stderr_file="$temp_dir/digest-mismatch.stderr"
  local json="$temp_dir/gh-digest-mismatch.json"
  local log="$temp_dir/gh-digest-mismatch.log"
  local fake_gh="$temp_dir/fake-gh-digest-mismatch"

  mkdir -p "$assets_dir" "$raw_dir"
  printf 'release bytes\n' > "$assets_dir/test-asset.tar.gz"
  write_digest_mismatch_json "$json"
  make_fake_gh "$fake_gh"

  if FAKE_GH_JSON="$json" \
    FAKE_GH_LOG="$log" \
    GH_BIN="$fake_gh" \
    ASSETS_DIR="$assets_dir" \
    OUT_SUMMARY="$summary" \
    OUT_RAW_DIR="$raw_dir" \
    REPO="Rul1an/assay" \
    SIGNER_WORKFLOW="Rul1an/assay/.github/workflows/release.yml" \
    SOURCE_REF="refs/tags/v9.9.9" \
    SOURCE_DIGEST="abc123" \
    bash "$SCRIPT" > /dev/null 2>"$stderr_file"; then
    echo "expected digest mismatch case to fail" >&2
    exit 1
  fi

  grep -F 'does not match the local subject digest' "$stderr_file" >/dev/null
}

run_missing_attestation_case() {
  local temp_dir="$1"
  local assets_dir="$temp_dir/assets-missing-attestation"
  local raw_dir="$temp_dir/raw-missing-attestation"
  local summary="$temp_dir/summary-missing-attestation.json"
  local stderr_file="$temp_dir/missing-attestation.stderr"
  local json="$temp_dir/gh-missing-attestation.json"
  local log="$temp_dir/gh-missing-attestation.log"
  local fake_gh="$temp_dir/fake-gh-missing-attestation"

  mkdir -p "$assets_dir" "$raw_dir"
  printf 'release bytes\n' > "$assets_dir/test-asset.tar.gz"
  write_empty_verification_json "$json"
  make_fake_gh "$fake_gh"

  if FAKE_GH_JSON="$json" \
    FAKE_GH_LOG="$log" \
    GH_BIN="$fake_gh" \
    ASSETS_DIR="$assets_dir" \
    OUT_SUMMARY="$summary" \
    OUT_RAW_DIR="$raw_dir" \
    REPO="Rul1an/assay" \
    SIGNER_WORKFLOW="Rul1an/assay/.github/workflows/release.yml" \
    SOURCE_REF="refs/tags/v9.9.9" \
    SOURCE_DIGEST="abc123" \
    bash "$SCRIPT" > /dev/null 2>"$stderr_file"; then
    echo "expected missing attestation case to fail" >&2
    exit 1
  fi

  grep -F 'No verified attestations returned' "$stderr_file" >/dev/null
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
  run_retry_success_case "$TEST_TEMP_DIR"
  run_missing_attestation_case "$TEST_TEMP_DIR"
  run_missing_witness_case "$TEST_TEMP_DIR"
  run_digest_mismatch_case "$TEST_TEMP_DIR"

  echo "release_attestation_enforce tests: PASS"
}

main "$@"
