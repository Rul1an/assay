#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

OUTDIR="$(mktemp -d)"
trap 'rm -rf "$OUTDIR"' EXIT

run_ok() {
  local infile="$1"
  local outjson="$2"
  local outmd="$3"

  bash scripts/ci/adr025-otel-bridge.sh \
    --in "$infile" \
    --out-json "$outjson" \
    --out-md "$outmd"

  test -f "$outjson"
  test -f "$outmd"
  python3 - <<'PY' "$outjson"
import json
import sys

d = json.load(open(sys.argv[1], "r", encoding="utf-8"))
assert d["schema_version"] == "otel_bridge_report_v1"
assert "traces" in d and isinstance(d["traces"], list)
PY
}

echo "[test] pass minimal"
run_ok "scripts/ci/fixtures/adr025-i3/otel_input_minimal.json" "$OUTDIR/r1.json" "$OUTDIR/r1.md"

echo "[test] uppercase ids normalize"
run_ok "scripts/ci/fixtures/adr025-i3/otel_input_uppercase_ids.json" "$OUTDIR/r2.json" "$OUTDIR/r2.md"
python3 - <<'PY' "$OUTDIR/r2.json"
import json
import sys

d = json.load(open(sys.argv[1], "r", encoding="utf-8"))
tid = d["traces"][0]["trace_id"]
assert tid == tid.lower()
PY

echo "[test] missing required => exit 2"
set +e
bash scripts/ci/adr025-otel-bridge.sh \
  --in "scripts/ci/fixtures/adr025-i3/otel_input_missing_required.json" \
  --out-json "$OUTDIR/r3.json" \
  --out-md "$OUTDIR/r3.md"
code=$?
set -e
test "$code" -eq 2

echo "[test] done"
