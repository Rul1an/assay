#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUT_DIR="/tmp/adr025_i2_out"
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

echo "[test] pass case -> exit 0"
bash scripts/ci/adr025-i2-closure-evaluate.sh \
  --soak scripts/ci/fixtures/adr025-i2/soak_report_minimal.json \
  --readiness scripts/ci/fixtures/adr025/readiness_pass.json \
  --manifest scripts/ci/fixtures/adr025-i2/manifest_full.json \
  --out-json "$OUT_DIR/pass.json" \
  --out-md "$OUT_DIR/pass.md"

python3 - <<'PY'
import json
from pathlib import Path
obj = json.loads(Path('/tmp/adr025_i2_out/pass.json').read_text(encoding='utf-8'))
assert obj['schema_version'] == 'closure_report_v1'
assert 'dimensions' in obj
assert 'score' in obj
print('pass artifact: ok')
PY

echo "[test] policy fail case -> exit 1"
set +e
bash scripts/ci/adr025-i2-closure-evaluate.sh \
  --soak scripts/ci/fixtures/adr025-i2/soak_report_minimal.json \
  --readiness scripts/ci/fixtures/adr025/readiness_policy_fail.json \
  --manifest scripts/ci/fixtures/adr025-i2/manifest_full.json \
  --out-json "$OUT_DIR/policy_fail.json" \
  --out-md "$OUT_DIR/policy_fail.md"
code=$?
set -e
test "$code" -eq 1

echo "[test] measurement fail case -> exit 2"
set +e
bash scripts/ci/adr025-i2-closure-evaluate.sh \
  --soak scripts/ci/fixtures/adr025-i2/soak_report_minimal.json \
  --readiness scripts/ci/fixtures/adr025-i2/readiness_invalid.json \
  --manifest scripts/ci/fixtures/adr025-i2/manifest_full.json \
  --out-json "$OUT_DIR/measurement_fail.json" \
  --out-md "$OUT_DIR/measurement_fail.md"
code=$?
set -e
test "$code" -eq 2

echo "[test] done"
