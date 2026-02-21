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

echo "[test] multi-trace ordering"
run_ok "scripts/ci/fixtures/adr025-i3/otel_input_multi_trace_unsorted.json" "$OUTDIR/r4.json" "$OUTDIR/r4.md"
python3 - <<'PY' "$OUTDIR/r4.json"
import json
import sys

d = json.load(open(sys.argv[1], "r", encoding="utf-8"))
tids = [t["trace_id"] for t in d["traces"]]
assert tids == sorted(tids), tids
PY

echo "[test] multi-span ordering + parent normalization + time strings + attribute sorting"
run_ok "scripts/ci/fixtures/adr025-i3/otel_input_multi_span_unsorted.json" "$OUTDIR/r5.json" "$OUTDIR/r5.md"
python3 - <<'PY' "$OUTDIR/r5.json"
import json
import sys

d = json.load(open(sys.argv[1], "r", encoding="utf-8"))
spans = d["traces"][0]["spans"]
ids = [s["span_id"] for s in spans]
assert ids == sorted(ids), ids

child = [s for s in spans if s["name"] == "child"][0]
assert child["parent_span_id"] == child["parent_span_id"].lower()

for s in spans:
    assert isinstance(s["start_time_unix_nano"], str) and s["start_time_unix_nano"].isdigit()
    assert isinstance(s["end_time_unix_nano"], str) and s["end_time_unix_nano"].isdigit()
    keys = [kv["key"] for kv in s["attributes"]]
    assert keys == sorted(keys), keys
PY

echo "[test] event ordering"
run_ok "scripts/ci/fixtures/adr025-i3/otel_input_events_unsorted.json" "$OUTDIR/r6.json" "$OUTDIR/r6.md"
python3 - <<'PY' "$OUTDIR/r6.json"
import json
import sys

d = json.load(open(sys.argv[1], "r", encoding="utf-8"))
events = d["traces"][0]["spans"][0]["events"]
pairs = [(e["time_unix_nano"], e["name"]) for e in events]
assert pairs == sorted(pairs), pairs

for e in events:
    keys = [kv["key"] for kv in e["attributes"]]
    assert keys == sorted(keys), keys
PY

echo "[test] link ordering"
run_ok "scripts/ci/fixtures/adr025-i3/otel_input_links_unsorted.json" "$OUTDIR/r7.json" "$OUTDIR/r7.md"
python3 - <<'PY' "$OUTDIR/r7.json"
import json
import sys

d = json.load(open(sys.argv[1], "r", encoding="utf-8"))
links = d["traces"][0]["spans"][0]["links"]
pairs = [(l["trace_id"], l["span_id"]) for l in links]
assert pairs == sorted(pairs), pairs

for l in links:
    keys = [kv["key"] for kv in l.get("attributes", [])]
    assert keys == sorted(keys), keys
PY

echo "[test] done"
