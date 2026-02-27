#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.lock"
  "crates/assay-adapter-api/src/lib.rs"
  "crates/assay-core/Cargo.toml"
  "crates/assay-core/src/attachments.rs"
  "crates/assay-core/src/lib.rs"
  "scripts/ci/review-adr026-stab-e2-b.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E2B must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E2B: $f"
    exit 1
  fi
done

echo "[review] host boundary markers"
rg -n 'Infrastructure' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: AdapterErrorKind::Infrastructure missing"
  exit 1
}
rg -n 'pub struct AttachmentWritePolicy' crates/assay-core/src/attachments.rs >/dev/null || {
  echo "FAIL: AttachmentWritePolicy missing"
  exit 1
}
rg -n 'pub struct FilesystemAttachmentWriter' crates/assay-core/src/attachments.rs >/dev/null || {
  echo "FAIL: FilesystemAttachmentWriter missing"
  exit 1
}
rg -n 'unsupported attachment media type' crates/assay-core/src/attachments.rs >/dev/null || {
  echo "FAIL: media-type policy enforcement missing"
  exit 1
}
rg -n 'payload exceeds attachment policy max_payload_bytes' crates/assay-core/src/attachments.rs >/dev/null || {
  echo "FAIL: size-cap policy enforcement missing"
  exit 1
}

cargo test -p assay-core attachment_writer >/dev/null

echo "[review] done"
