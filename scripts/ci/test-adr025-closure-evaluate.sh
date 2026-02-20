#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUTDIR="$(mktemp -d)"
trap 'rm -rf "$OUTDIR"' EXIT

POLICY="schemas/closure_policy_v1.json"
SOAK="scripts/ci/fixtures/adr025-i2/soak_minimal.json"
READY="scripts/ci/fixtures/adr025-i2/readiness_pass.json"

echo "[test] pass case (manifest good)"
bash scripts/ci/adr025-closure-evaluate.sh \
  --soak "$SOAK" --readiness "$READY" --manifest "scripts/ci/fixtures/adr025-i2/manifest_good.json" \
  --policy "$POLICY" --out-json "$OUTDIR/closure.json" --out-md "$OUTDIR/closure.md"
test -f "$OUTDIR/closure.json"
test -f "$OUTDIR/closure.md"

echo "[test] policy/closure fail (manifest missing provenance => low score)"
set +e
bash scripts/ci/adr025-closure-evaluate.sh \
  --soak "$SOAK" --readiness "$READY" --manifest "scripts/ci/fixtures/adr025-i2/manifest_missing_provenance.json" \
  --policy "$POLICY" --out-json "$OUTDIR/closure_fail.json" --out-md "$OUTDIR/closure_fail.md"
code=$?
set -e
test "$code" -eq 1

echo "[test] measurement fail (missing readiness file)"
set +e
bash scripts/ci/adr025-closure-evaluate.sh \
  --soak "$SOAK" --readiness "$OUTDIR/nope.json" --manifest "scripts/ci/fixtures/adr025-i2/manifest_good.json" \
  --policy "$POLICY" --out-json "$OUTDIR/x.json" --out-md "$OUTDIR/x.md"
code=$?
set -e
test "$code" -eq 2

echo "[test] done"
