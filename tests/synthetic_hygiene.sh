#!/bin/bash
set -e

# Path to verdict binary
VERDICT_BIN="../target/debug/verdict"

if [ ! -f "$VERDICT_BIN" ]; then
    echo "Error: verdict binary not found at $VERDICT_BIN"
    exit 1
fi

DB="synthetic_hygiene.db"
rm -f $DB

echo "Creating synthetic database..."
sqlite3 $DB "
CREATE TABLE runs (
    id INTEGER PRIMARY KEY,
    suite TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    metadata_json TEXT
);
CREATE TABLE results (
    id INTEGER PRIMARY KEY,
    run_id INTEGER NOT NULL,
    test_id TEXT NOT NULL,
    outcome TEXT NOT NULL,
    message TEXT,
    duration_ms INTEGER,
    output_json TEXT,
    score REAL,
    fingerprint TEXT,
    skip_reason TEXT,
    attempts_json TEXT, -- required for ingestion but can be empty
    FOREIGN KEY(run_id) REFERENCES runs(id)
);
"

SUITE="synth-suite"
TEST_ID="test-synth"

# Helper to construct attempts JSON
# AttemptRow: { attempt_no: 1, status: "fail", message: "...", duration_ms: 100, details: { metrics: { faithfulness: { score: 0.1, reason: "hallucination detected" } } } }
ATTEMPT_FAIL_JSON='[{"attempt_no":1,"status":"fail","message":"hallucination","duration_ms":100,"details":{"metrics":{"faithfulness":{"score":0.1,"reason":"hallucination detected"}}}}]'
ATTEMPT_PASS_JSON='[{"attempt_no":1,"status":"pass","message":"","duration_ms":100,"details":{"metrics":{"faithfulness":{"score":1.0,"reason":"perfect"}}}}]'

sqlite3 $DB <<EOF
INSERT INTO runs (id, suite) VALUES (1, '$SUITE');
INSERT INTO results (run_id, test_id, outcome, score, attempts_json, output_json) VALUES (1, '$TEST_ID', 'pass', 1.0, '$ATTEMPT_PASS_JSON', '{}');

INSERT INTO runs (id, suite) VALUES (2, '$SUITE');
INSERT INTO results (run_id, test_id, outcome, score, attempts_json, message, output_json) VALUES (2, '$TEST_ID', 'fail', 0.0, '$ATTEMPT_FAIL_JSON', 'hallucination', '{}');

INSERT INTO runs (id, suite) VALUES (3, '$SUITE');
INSERT INTO results (run_id, test_id, outcome, score, attempts_json, output_json) VALUES (3, '$TEST_ID', 'flaky', 0.5, '[]', '{}');

INSERT INTO runs (id, suite) VALUES (4, '$SUITE');
INSERT INTO results (run_id, test_id, outcome, score, attempts_json, skip_reason, output_json) VALUES (4, '$TEST_ID', 'pass', 1.0, '[]', 'fingerprint match', '{}');

INSERT INTO runs (id, suite) VALUES (5, '$SUITE');
INSERT INTO results (run_id, test_id, outcome, score, attempts_json, output_json) VALUES (5, '$TEST_ID', 'pass', 1.0, '$ATTEMPT_PASS_JSON', '{}');
EOF

echo "Running verdict baseline report..."
$VERDICT_BIN baseline report --db $DB --suite $SUITE --last 10 --out hygiene_synth.json

echo "Inspecting results..."
cat hygiene_synth.json

# Assertions

check_rate() {
    rate_name=$1
    expected=$2
    actual=$(grep "\"$rate_name\":" hygiene_synth.json | head -n 1 | awk '{print $2}' | tr -d ',')

    # Use python for robust float comparison
    if python3 -c "import sys; expected=float($expected); actual=float($actual); sys.exit(0 if abs(actual - expected) < 0.001 else 1)"; then
        echo "✅ $rate_name rate matches $expected (got $actual)"
    else
        echo "❌ $rate_name rate MISMATCH (expected $expected, got $actual)"
        exit 1
    fi
}

check_rate "pass" "0.6"
check_rate "fail" "0.2"
check_rate "flaky" "0.2"
check_rate "skipped" "0.2"

# Hardening Assertions
if grep -q "\"score_source\": \"all_attempts\"" hygiene_synth.json; then
    echo "✅ score_source is all_attempts"
else
    echo "❌ score_source check failed"
    exit 1
fi

# Check Top Reasons (should contain "hallucination detected")
if grep -q "hallucination detected" hygiene_synth.json; then
    echo "✅ metric reason found in top_reasons"
else
    echo "❌ metric reason collection failed"
    exit 1
fi

# Check Suggested Actions (P10 < 0.6 due to fail score 0.1)
if grep -q "Low faithfulness scores" hygiene_synth.json; then
    echo "✅ suggested action found for low scores"
else
    echo "❌ suggested action check failed"
    exit 1
fi

echo "Integration Test Passed!"
rm $DB hygiene_synth.json
