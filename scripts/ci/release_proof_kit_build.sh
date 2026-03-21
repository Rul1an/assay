#!/usr/bin/env bash
set -euo pipefail

GH_BIN="${GH_BIN:-gh}"
JQ_BIN="${JQ_BIN:-jq}"
ASSETS_DIR="${ASSETS_DIR:?ASSETS_DIR is required}"
PROVENANCE_SUMMARY="${PROVENANCE_SUMMARY:?PROVENANCE_SUMMARY is required}"
PROVENANCE_SUMMARY_SHA256="${PROVENANCE_SUMMARY_SHA256:?PROVENANCE_SUMMARY_SHA256 is required}"
OUT_ARCHIVE="${OUT_ARCHIVE:?OUT_ARCHIVE is required}"
VERSION="${VERSION:?VERSION is required}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INVENTORY_SCRIPT="${INVENTORY_SCRIPT:-${SCRIPT_DIR}/release_archive_inventory.sh}"

require_bin() {
  local bin="$1"
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "missing required binary: $bin" >&2
    exit 1
  fi
}

json_get() {
  local expr="$1"
  "$JQ_BIN" -er "$expr" "$PROVENANCE_SUMMARY"
}

write_verify_offline() {
  local path="$1"
  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

GH_BIN="${GH_BIN:-gh}"
JQ_BIN="${JQ_BIN:-jq}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="${ROOT}/manifest.json"

usage() {
  echo "usage: verify-offline.sh --assets-dir <dir>" >&2
  exit 1
}

require_bin() {
  local bin="$1"
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "missing required binary: $bin" >&2
    exit 1
  fi
}

assets_dir=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --assets-dir)
      [ "$#" -ge 2 ] || usage
      assets_dir="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[ -n "$assets_dir" ] || usage
[ -d "$assets_dir" ] || {
  echo "assets directory not found: $assets_dir" >&2
  exit 1
}

require_bin "$GH_BIN"
require_bin "$JQ_BIN"

[ -f "$MANIFEST" ] || {
  echo "manifest not found: $MANIFEST" >&2
  exit 1
}

trusted_root_rel="$("$JQ_BIN" -er '.trusted_root_path' "$MANIFEST")"
trusted_root_path="${ROOT}/${trusted_root_rel}"
[ -f "$trusted_root_path" ] || {
  echo "trusted root not found: $trusted_root_path" >&2
  exit 1
}

asset_names=()
while IFS= read -r asset_name; do
  asset_names+=("$asset_name")
done < <("$JQ_BIN" -er '.assets[].name' "$MANIFEST")
if [ "${#asset_names[@]}" -eq 0 ]; then
  echo "manifest contains no assets" >&2
  exit 1
fi

repo="$("$JQ_BIN" -er '.repo' "$MANIFEST")"
signer_workflow="$("$JQ_BIN" -er '.signer_workflow' "$MANIFEST")"
cert_oidc_issuer="$("$JQ_BIN" -er '.cert_oidc_issuer' "$MANIFEST")"
source_ref="$("$JQ_BIN" -er '.source_ref' "$MANIFEST")"
source_digest="$("$JQ_BIN" -er '.source_digest' "$MANIFEST")"
predicate_type="$("$JQ_BIN" -er '.predicate_type' "$MANIFEST")"
deny_self_hosted="$("$JQ_BIN" -er '.deny_self_hosted_runners' "$MANIFEST")"

if [ "$deny_self_hosted" != "true" ]; then
  echo "manifest deny_self_hosted_runners must be true" >&2
  exit 1
fi

for asset_name in "${asset_names[@]}"; do
  asset_path="${assets_dir}/${asset_name}"
  [ -f "$asset_path" ] || {
    echo "asset not found for offline verification: $asset_path" >&2
    exit 1
  }

  bundle_rel="$("$JQ_BIN" -er --arg name "$asset_name" '.assets[] | select(.name == $name) | .bundle_path' "$MANIFEST")"
  bundle_path="${ROOT}/${bundle_rel}"
  [ -f "$bundle_path" ] || {
    echo "bundle not found for asset ${asset_name}: $bundle_path" >&2
    exit 1
  }

  "$GH_BIN" attestation verify "$asset_path" \
    --repo "$repo" \
    --signer-workflow "$signer_workflow" \
    --cert-oidc-issuer "$cert_oidc_issuer" \
    --source-ref "$source_ref" \
    --source-digest "$source_digest" \
    --predicate-type "$predicate_type" \
    --deny-self-hosted-runners \
    --bundle "$bundle_path" \
    --custom-trusted-root "$trusted_root_path"
done
EOF
  chmod +x "$path"
}

write_verify_release_online() {
  local path="$1"
  cat >"$path" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

# convenience-only helper. The canonical verification path for this kit is
# verify-offline.sh with the shipped bundles and trusted root snapshot.

GH_BIN="${GH_BIN:-gh}"
JQ_BIN="${JQ_BIN:-jq}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MANIFEST="${ROOT}/manifest.json"

usage() {
  echo "usage: verify-release-online.sh --assets-dir <dir>" >&2
  exit 1
}

require_bin() {
  local bin="$1"
  if ! command -v "$bin" >/dev/null 2>&1; then
    echo "missing required binary: $bin" >&2
    exit 1
  fi
}

assets_dir=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --assets-dir)
      [ "$#" -ge 2 ] || usage
      assets_dir="$2"
      shift 2
      ;;
    *)
      usage
      ;;
  esac
done

[ -n "$assets_dir" ] || usage
[ -d "$assets_dir" ] || {
  echo "assets directory not found: $assets_dir" >&2
  exit 1
}

require_bin "$GH_BIN"
require_bin "$JQ_BIN"

repo="$("$JQ_BIN" -er '.repo' "$MANIFEST")"
tag="$("$JQ_BIN" -er '.tag' "$MANIFEST")"

"$GH_BIN" release verify "$tag" -R "$repo"

asset_names=()
while IFS= read -r asset_name; do
  asset_names+=("$asset_name")
done < <("$JQ_BIN" -er '.assets[].name' "$MANIFEST")
for asset_name in "${asset_names[@]}"; do
  asset_path="${assets_dir}/${asset_name}"
  [ -f "$asset_path" ] || {
    echo "asset not found for online verification: $asset_path" >&2
    exit 1
  }
  "$GH_BIN" release verify-asset "$tag" "$asset_path" -R "$repo"
done
EOF
  chmod +x "$path"
}

write_readme() {
  local path="$1"
  local gh_version="$2"
  cat >"$path" <<EOF
# Assay Release Proof Kit

This proof kit is the canonical offline verification path for this Assay release.
It packages the exact provenance policy already enforced in CI, together with the
attestation bundle(s) and trusted-root snapshot needed to reproduce verification.

## Contents

- \`manifest.json\`: consumer verification contract derived from \`release-provenance.json\`
- \`release-provenance.json\` and \`release-provenance.json.sha256\`
- \`trusted_root.jsonl\`: raw output from \`gh attestation trusted-root\`
- \`bundles/*.jsonl\`: per-asset attestation bundles downloaded with \`gh attestation download\`
- \`verify-offline.sh\`: canonical offline verification helper
- \`verify-release-online.sh\`: convenience helper around \`gh release verify\`

## Requirements

- GitHub CLI with support for:
  - \`gh attestation trusted-root\`
  - \`gh attestation download\`
  - \`gh attestation verify --bundle --custom-trusted-root\`
  - \`gh release verify\`
  - \`gh release verify-asset\`
- \`jq\`

Tested with GitHub CLI ${gh_version}.

## Offline Verification (Canonical)

1. Download the release archive(s) referenced in \`manifest.json\`.
2. Unpack this proof kit.
3. Run:

\`\`\`bash
./verify-offline.sh --assets-dir /path/to/release-assets
\`\`\`

The helper fails closed if the manifest, trusted root, bundle, or any listed
asset is missing.

## Online Verification (Convenience Only)

If you want an online cross-check against GitHub's release APIs, run:

\`\`\`bash
./verify-release-online.sh --assets-dir /path/to/release-assets
\`\`\`

This helper is convenience-only. The canonical verification path for this kit is
\`verify-offline.sh\`.

## Trust Boundary

- This kit verifies artifacts within the GitHub artifact-attestation model.
- \`trusted_root.jsonl\` is a snapshot captured when this kit was built.
- \`trusted_root_generated_at\` in the manifest is not a statement of permanent
  validity; it records when the snapshot was captured.
- Refresh the trusted-root snapshot whenever you import newer signed material
  into an offline environment.

## Non-Goals

This kit does not provide:

- general Sigstore verification
- generic Rekor verification
- a complete supply-chain guarantee
- runtime trust enforcement
- a guarantee beyond the GitHub artifact-attestation model
EOF
}

require_bin "$GH_BIN"
require_bin "$JQ_BIN"
require_bin "$INVENTORY_SCRIPT"

[ -f "$PROVENANCE_SUMMARY" ] || {
  echo "provenance summary not found: $PROVENANCE_SUMMARY" >&2
  exit 1
}
[ -f "$PROVENANCE_SUMMARY_SHA256" ] || {
  echo "provenance summary checksum not found: $PROVENANCE_SUMMARY_SHA256" >&2
  exit 1
}

repo="$(json_get '.verification_policy.repo')"
predicate_type="$(json_get '.verification_policy.predicate_type')"
deny_self_hosted_runners="$(json_get '.verification_policy.deny_self_hosted_runners')"

if [ "$deny_self_hosted_runners" != "true" ]; then
  echo "release provenance summary must set deny_self_hosted_runners to true" >&2
  exit 1
fi

summary_assets=()
while IFS= read -r asset_name; do
  summary_assets+=("$asset_name")
done < <("$JQ_BIN" -er '.assets[].name' "$PROVENANCE_SUMMARY")
if [ "${#summary_assets[@]}" -eq 0 ]; then
  echo "release provenance summary contains no assets" >&2
  exit 1
fi

inventory_paths=()
while IFS= read -r asset_path; do
  inventory_paths+=("$asset_path")
done < <("$INVENTORY_SCRIPT" "$ASSETS_DIR")
if [ "${#inventory_paths[@]}" -eq 0 ]; then
  echo "No release archives found for proof kit build" >&2
  exit 1
fi

inventory_assets=()
for asset_path in "${inventory_paths[@]}"; do
  inventory_assets+=("$(basename "$asset_path")")
done

summary_joined="$(printf '%s\n' "${summary_assets[@]}")"
inventory_joined="$(printf '%s\n' "${inventory_assets[@]}")"
if [ "$summary_joined" != "$inventory_joined" ]; then
  echo "proof kit asset set does not match S1 provenance summary" >&2
  exit 1
fi

scratch_dir="$(mktemp -d)"
trap 'rm -rf "$scratch_dir"' EXIT
kit_root="${scratch_dir}/release-proof-kit"
mkdir -p "${kit_root}/bundles"

trusted_root_path="${kit_root}/trusted_root.jsonl"
if ! "$GH_BIN" attestation trusted-root >"$trusted_root_path"; then
  echo "failed to download trusted root snapshot" >&2
  exit 1
fi
if [ ! -s "$trusted_root_path" ]; then
  echo "trusted root snapshot is empty" >&2
  exit 1
fi

for asset_name in "${summary_assets[@]}"; do
  asset_path="${ASSETS_DIR}/${asset_name}"
  [ -f "$asset_path" ] || {
    echo "asset listed in provenance summary not found: $asset_path" >&2
    exit 1
  }

  download_dir="${scratch_dir}/downloads/${asset_name}"
  mkdir -p "$download_dir"
  if ! (
    cd "$download_dir"
    "$GH_BIN" attestation download "$asset_path" --repo "$repo" --predicate-type "$predicate_type" >/dev/null
  ); then
    echo "failed to download attestation bundle for ${asset_name}" >&2
    exit 1
  fi

  downloaded_bundles=()
  while IFS= read -r bundle_path; do
    downloaded_bundles+=("$bundle_path")
  done < <(find "$download_dir" -maxdepth 1 -type f -name '*.jsonl' -print | sort)
  if [ "${#downloaded_bundles[@]}" -ne 1 ]; then
    echo "expected exactly one bundle file for ${asset_name}, found ${#downloaded_bundles[@]}" >&2
    exit 1
  fi

  bundle_dest="${kit_root}/bundles/${asset_name}.jsonl"
  cp "${downloaded_bundles[0]}" "$bundle_dest"
  if [ ! -s "$bundle_dest" ]; then
    echo "downloaded bundle is empty for ${asset_name}" >&2
    exit 1
  fi
done

cp "$PROVENANCE_SUMMARY" "${kit_root}/release-provenance.json"
cp "$PROVENANCE_SUMMARY_SHA256" "${kit_root}/release-provenance.json.sha256"

trusted_root_generated_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
"$JQ_BIN" \
  --arg version "$VERSION" \
  --arg trusted_root_path "trusted_root.jsonl" \
  --arg trusted_root_generated_at "$trusted_root_generated_at" \
  --arg release_provenance_path "release-provenance.json" \
  --arg release_provenance_sha256_path "release-provenance.json.sha256" '
  {
    schema_version: 1,
    tag: $version,
    repo: .verification_policy.repo,
    signer_workflow: .verification_policy.signer_workflow,
    cert_oidc_issuer: .verification_policy.cert_oidc_issuer,
    source_ref: .verification_policy.source_ref,
    source_digest: .verification_policy.source_digest,
    predicate_type: .verification_policy.predicate_type,
    deny_self_hosted_runners: .verification_policy.deny_self_hosted_runners,
    trusted_root_path: $trusted_root_path,
    trusted_root_generated_at: $trusted_root_generated_at,
    release_provenance_path: $release_provenance_path,
    release_provenance_sha256_path: $release_provenance_sha256_path,
    assets: [
      .assets[] |
      {
        name,
        sha256,
        bundle_path: ("bundles/" + .name + ".jsonl")
      }
    ]
  }
' "$PROVENANCE_SUMMARY" > "${kit_root}/manifest.json"

gh_version="$("$GH_BIN" --version | head -n1 | awk '{print $3}')"
write_readme "${kit_root}/README.md" "$gh_version"
write_verify_offline "${kit_root}/verify-offline.sh"
write_verify_release_online "${kit_root}/verify-release-online.sh"

mkdir -p "$(dirname "$OUT_ARCHIVE")"
tar -czf "$OUT_ARCHIVE" -C "$scratch_dir" "$(basename "$kit_root")"
