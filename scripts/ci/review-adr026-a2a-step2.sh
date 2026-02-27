#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-a2a-step1-freeze}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.lock"
  "crates/assay-adapter-a2a/Cargo.toml"
  "crates/assay-adapter-a2a/src/lib.rs"
  "scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_agent_capabilities.json"
  "scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_task_requested.json"
  "scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_artifact_shared.json"
  "scripts/ci/fixtures/adr026/a2a/v0.2/a2a_negative_missing_task_id.json"
  "scripts/ci/fixtures/adr026/a2a/v0.2/a2a_negative_invalid_event_type.json"
  "scripts/ci/fixtures/adr026/a2a/v0.2/a2a_negative_malformed.json"
  "scripts/ci/test-adapter-a2a.sh"
  "scripts/ci/review-adr026-a2a-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 A2A Step2 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 A2A Step2: $f"
    exit 1
  fi
done

echo "[review] A2A contract markers"
rg -n '^name = "assay-adapter-a2a"$' crates/assay-adapter-a2a/Cargo.toml >/dev/null || {
  echo "FAIL: missing assay-adapter-a2a crate"
  exit 1
}
rg -n 'pub struct A2aAdapter' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: missing A2aAdapter type"
  exit 1
}
rg -n 'ConvertMode::Lenient' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: missing lenient-mode handling"
  exit 1
}
rg -n 'StrictLossinessViolation' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: strict lossiness contract missing"
  exit 1
}
rg -n 'assay.adapter.a2a.task.requested' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: missing task.requested mapping"
  exit 1
}

bash scripts/ci/test-adapter-a2a.sh >/dev/null

echo "[review] done"
