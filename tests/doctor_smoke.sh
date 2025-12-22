#!/usr/bin/env bash
set -euo pipefail

# Build first
cargo build -q --bin verdict

BIN="./target/debug/verdict"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

cat > "$TMP/eval.yaml" <<'YAML'
version: 1
suite: "doctor-smoke"
model: "trace"
settings: { cache: false }
tests:
  - id: "t1"
    input: { prompt: "p1" }
    expected:
      type: must_contain
      must_contain: ["hello"]
YAML

cat > "$TMP/trace.jsonl" <<'JSONL'
{"schema_version":1,"type":"verdict.trace","request_id":"1","prompt":"p1","response":"hello world","model":"trace","provider":"trace","meta":{}}
JSONL

echo "Running verdict doctor..."
"$BIN" doctor --config "$TMP/eval.yaml" --trace-file "$TMP/trace.jsonl" --format json --out "$TMP/doctor.json"

cat "$TMP/doctor.json"

grep -q '"schema_version": 1' "$TMP/doctor.json"
grep -q '"diagnostics"' "$TMP/doctor.json"
grep -q '"suggested_actions"' "$TMP/doctor.json"

echo "✅ doctor smoke passed"
