#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.toml"
  "crates/assay-adapter-api/Cargo.toml"
  "crates/assay-adapter-api/src/lib.rs"
  "docs/architecture/ADR-026-Protocol-Adapters.md"
  "docs/architecture/ADR-026-Adjacent-Notes.md"
  "scripts/ci/review-adr026-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 Step1 must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 Step1: $f"
    exit 1
  fi
done

echo "[review] crate contract markers"
rg -n '^name = "assay-adapter-api"$' crates/assay-adapter-api/Cargo.toml >/dev/null || {
  echo "FAIL: missing assay-adapter-api crate"
  exit 1
}
rg -n 'pub trait ProtocolAdapter' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: adapter trait missing"
  exit 1
}
rg -n 'pub trait AttachmentWriter' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: attachment writer contract missing"
  exit 1
}
rg -n 'enum ConvertMode' crates/assay-adapter-api/src/lib.rs >/dev/null || {
  echo "FAIL: convert mode contract missing"
  exit 1
}

echo "[review] ADR markers"
rg -n '^# ADR-026: Protocol Adapters \(Adapter-First Strategy\)$' docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 title mismatch"
  exit 1
}
rg -n '^## Versioning and Conformance$' docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 missing conformance section"
  exit 1
}
rg -n 'negative fixture per supported protocol version' docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 must require negative fixtures"
  exit 1
}
rg -n '^## Why ACP first$' docs/architecture/ADR-026-Adjacent-Notes.md >/dev/null || {
  echo "FAIL: ADR-026 adjacent notes missing ACP rationale"
  exit 1
}
rg -n '^## Why A2A second$' docs/architecture/ADR-026-Adjacent-Notes.md >/dev/null || {
  echo "FAIL: ADR-026 adjacent notes missing A2A sequencing"
  exit 1
}

cargo test -p assay-adapter-api >/dev/null

echo "[review] done"
