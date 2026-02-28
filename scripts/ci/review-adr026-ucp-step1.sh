#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.toml"
  "Cargo.lock"
  "crates/assay-adapter-ucp/Cargo.toml"
  "crates/assay-adapter-ucp/src/lib.rs"
  "docs/architecture/PLAN-ADR-026-UCP-2026q2.md"
  "scripts/ci/review-adr026-ucp-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 UCP Step1 must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 UCP Step1: $f"
    exit 1
  fi
done

echo "[review] contract markers"
rg -n '^name = "assay-adapter-ucp"$' crates/assay-adapter-ucp/Cargo.toml >/dev/null || {
  echo "FAIL: missing assay-adapter-ucp crate"
  exit 1
}
rg -n 'pub struct UcpAdapter' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing UcpAdapter type"
  exit 1
}
rg -n 'fn adapter\(&self\) -> AdapterDescriptor' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing adapter metadata contract"
  exit 1
}
rg -n 'runtime translation is not implemented yet for UCP Step1' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: stubbed Step1 convert contract missing"
  exit 1
}
rg -n '^# PLAN — ADR-026 UCP Adapter Follow-up \(2026q2\)$' docs/architecture/PLAN-ADR-026-UCP-2026q2.md >/dev/null || {
  echo "FAIL: missing UCP plan title"
  exit 1
}
rg -n '^## Initial event families \(frozen for Step2\)$' docs/architecture/PLAN-ADR-026-UCP-2026q2.md >/dev/null || {
  echo "FAIL: missing initial event families section"
  exit 1
}
rg -n '^## Upstream version anchor \(frozen\)$' docs/architecture/PLAN-ADR-026-UCP-2026q2.md >/dev/null || {
  echo "FAIL: missing upstream version anchor section"
  exit 1
}

cargo test -p assay-adapter-ucp >/dev/null

echo "[review] done"
