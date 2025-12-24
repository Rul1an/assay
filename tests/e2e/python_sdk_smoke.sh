#!/bin/bash
set -euo pipefail

# Build Verdict
cargo build --bin verdict --release --quiet
VERDICT=$PWD/target/release/verdict

TRACE_FILE="/tmp/assay_trace.jsonl"
CONFIG_FILE="/tmp/assay_eval.yaml"

rm -f "$TRACE_FILE"

echo "Generating trace via Python SDK..."

PYTHONPATH=assay/python python3 - <<'PY'
from assay import TraceWriter, EpisodeRecorder
w = TraceWriter("/tmp/assay_trace.jsonl")
with EpisodeRecorder(writer=w, episode_id="mcp_demo", test_id="mcp_demo", prompt="demo_user_prompt") as ep:
    sid = ep.step(kind="model", name="agent", content="ok")
    ep.tool_call(tool_name="ApplyDiscount", args={"percent":50}, result={"value":"denied"}, step_id=sid)
    ep.end(outcome="pass")
PY

echo "Generated trace:"
cat "$TRACE_FILE"

echo "Creating config..."

cat > "$CONFIG_FILE" <<'YAML'
version: 1
suite: "sdk_smoke"
model: "trace"
tests:
  - id: mcp_demo
    description: "must verify ApplyDiscount was called"
    input:
      prompt: "demo_user_prompt"
    expected:
      type: regex_match
      pattern: ".*"
    assertions:
      - type: trace_must_call_tool
        tool: ApplyDiscount
YAML

DB_FILE="/tmp/smoke.db"
rm -f "$DB_FILE"

echo "Running Verdict CI Gate (DB: $DB_FILE)..."

set +e
$VERDICT ci --config "$CONFIG_FILE" --trace-file "$TRACE_FILE" --db "$DB_FILE" --replay-strict
EXIT_CODE=$?
set -e

if [ $EXIT_CODE -ne 0 ]; then
    echo "❌ CI Gate Failed"
    echo "Inspecting DB..."
    python3 -c "
import sqlite3
conn = sqlite3.connect('$DB_FILE')
c = conn.cursor()
c.execute('SELECT id, test_id, prompt FROM episodes')
rows = c.fetchall()
print('Episodes in DB:', rows)
"
    exit $EXIT_CODE
else
    echo "✅ Python SDK Smoke Test Passed"
fi
