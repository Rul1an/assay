#!/usr/bin/env bash
set -euo pipefail

GH_BIN="${GH_BIN:-gh}"
JQ_BIN="${JQ_BIN:-jq}"
ASSETS_DIR="${ASSETS_DIR:?ASSETS_DIR is required}"
OUT_SUMMARY="${OUT_SUMMARY:?OUT_SUMMARY is required}"
OUT_RAW_DIR="${OUT_RAW_DIR:?OUT_RAW_DIR is required}"
REPO="${REPO:?REPO is required}"
SIGNER_WORKFLOW="${SIGNER_WORKFLOW:?SIGNER_WORKFLOW is required}"
SOURCE_REF="${SOURCE_REF:?SOURCE_REF is required}"
SOURCE_DIGEST="${SOURCE_DIGEST:?SOURCE_DIGEST is required}"
CERT_OIDC_ISSUER="${CERT_OIDC_ISSUER:-https://token.actions.githubusercontent.com}"
PREDICATE_TYPE="${PREDICATE_TYPE:-https://slsa.dev/provenance/v1}"

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

require_bin() {
  local bin="$1"
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "missing required binary: $bin" >&2
    exit 1
  fi
}

require_bin "$GH_BIN"
require_bin "$JQ_BIN"

mkdir -p "$OUT_RAW_DIR"
mkdir -p "$(dirname "$OUT_SUMMARY")"

assets=()
while IFS= read -r asset; do
  assets+=("$asset")
done < <(find "$ASSETS_DIR" -maxdepth 1 -type f \( -name '*.tar.gz' -o -name '*.zip' \) -print | sort)

if [ "${#assets[@]}" -eq 0 ]; then
  echo "No release archives found for attestation verification" >&2
  exit 1
fi

scratch_dir="$(mktemp -d)"
trap 'rm -rf "$scratch_dir"' EXIT
asset_summaries_jsonl="$scratch_dir/asset-summaries.jsonl"
: > "$asset_summaries_jsonl"

for asset in "${assets[@]}"; do
  asset_name="$(basename "$asset")"
  raw_file="$OUT_RAW_DIR/${asset_name}.attestation.json"
  asset_sha256="$(compute_sha256 "$asset")"

  verify_json="$("$GH_BIN" attestation verify "$asset" \
    --repo "$REPO" \
    --signer-workflow "$SIGNER_WORKFLOW" \
    --cert-oidc-issuer "$CERT_OIDC_ISSUER" \
    --predicate-type "$PREDICATE_TYPE" \
    --source-digest "$SOURCE_DIGEST" \
    --source-ref "$SOURCE_REF" \
    --deny-self-hosted-runners \
    --format json)"

  printf '%s\n' "$verify_json" > "$raw_file"

  if ! printf '%s\n' "$verify_json" | "$JQ_BIN" -e 'length > 0' >/dev/null; then
    echo "No verified attestations returned for ${asset_name}" >&2
    exit 1
  fi

  if ! printf '%s\n' "$verify_json" | "$JQ_BIN" -e '
    all(.[]; ((.verificationResult.verifiedTimestamps // []) | length) > 0)
  ' >/dev/null; then
    echo "Verified attestation for ${asset_name} is missing transparency/timestamp witnesses" >&2
    exit 1
  fi

  if ! printf '%s\n' "$verify_json" | "$JQ_BIN" -e --arg digest "$asset_sha256" '
    any(.[]; any((.verificationResult.statement.subject // [])[]?; .digest.sha256? == $digest))
  ' >/dev/null; then
    echo "Verified attestation for ${asset_name} does not match the local subject digest" >&2
    exit 1
  fi

  printf '%s\n' "$verify_json" | "$JQ_BIN" -c \
    --arg name "$asset_name" \
    --arg sha256 "$asset_sha256" \
    --arg raw_file "$(basename "$raw_file")" '
      {
        name: $name,
        sha256: $sha256,
        raw_attestation_file: $raw_file,
        verified_attestations: length,
        predicate_types: ([.[].verificationResult.statement.predicateType // empty] | unique | sort),
        verified_timestamp_count: ([.[].verificationResult.verifiedTimestamps[]?] | length),
        subjects: ([.[].verificationResult.statement.subject[]? | {name, digest}] | unique)
      }
    ' >> "$asset_summaries_jsonl"
done

"$JQ_BIN" -s \
  --arg repo "$REPO" \
  --arg signer_workflow "$SIGNER_WORKFLOW" \
  --arg cert_oidc_issuer "$CERT_OIDC_ISSUER" \
  --arg source_ref "$SOURCE_REF" \
  --arg source_digest "$SOURCE_DIGEST" \
  --arg predicate_type "$PREDICATE_TYPE" '
  {
    schema_version: 1,
    verification_policy: {
      repo: $repo,
      signer_workflow: $signer_workflow,
      cert_oidc_issuer: $cert_oidc_issuer,
      source_ref: $source_ref,
      source_digest: $source_digest,
      predicate_type: $predicate_type,
      deny_self_hosted_runners: true
    },
    assets: (sort_by(.name))
  }
' "$asset_summaries_jsonl" > "$OUT_SUMMARY"
