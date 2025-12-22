pub const DDL: &str = r#"
CREATE TABLE IF NOT EXISTS runs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  suite TEXT NOT NULL,
  started_at TEXT NOT NULL,
  status TEXT NOT NULL,
  config_json TEXT
);

CREATE TABLE IF NOT EXISTS results (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  run_id INTEGER NOT NULL REFERENCES runs(id),
  test_id TEXT NOT NULL,
  outcome TEXT NOT NULL,
  score REAL,
  duration_ms INTEGER,
  attempts_json TEXT,
  output_json TEXT,
  fingerprint TEXT,
  skip_reason TEXT
);

CREATE TABLE IF NOT EXISTS attempts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  result_id INTEGER NOT NULL REFERENCES results(id),
  attempt_number INTEGER NOT NULL,
  outcome TEXT NOT NULL,
  score REAL,
  duration_ms INTEGER,
  output_json TEXT,
  error_message TEXT
);

CREATE TABLE IF NOT EXISTS quarantine (
  suite TEXT NOT NULL,
  test_id TEXT NOT NULL,
  reason TEXT,
  added_at TEXT NOT NULL,
  PRIMARY KEY (suite, test_id)
);

CREATE TABLE IF NOT EXISTS cache (
  key TEXT PRIMARY KEY,
  response_json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS embeddings (
  key TEXT PRIMARY KEY,
  model TEXT NOT NULL,
  dims INTEGER NOT NULL,
  vec BLOB NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS judge_cache (
  key TEXT PRIMARY KEY,
  provider TEXT NOT NULL,
  model TEXT NOT NULL,
  rubric_id TEXT NOT NULL,
  rubric_version TEXT NOT NULL,
  created_at TEXT NOT NULL,
  payload_json TEXT NOT NULL
);
"#;
