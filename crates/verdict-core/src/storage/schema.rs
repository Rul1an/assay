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

-- Trace V2
CREATE TABLE IF NOT EXISTS episodes (
    id TEXT PRIMARY KEY,
    run_id INTEGER, -- Optional for loose traces, required for CI
    test_id TEXT, -- Optional
    timestamp INTEGER NOT NULL,
    prompt TEXT,
    outcome TEXT,
    meta_json TEXT,
    FOREIGN KEY(run_id) REFERENCES runs(id)
);

CREATE TABLE IF NOT EXISTS steps (
    id TEXT PRIMARY KEY,
    episode_id TEXT NOT NULL,
    idx INTEGER NOT NULL,
    kind TEXT,
    name TEXT,
    content TEXT,
    content_sha256 TEXT,
    truncations_json TEXT,
    meta_json TEXT,
    FOREIGN KEY(episode_id) REFERENCES episodes(id),
    UNIQUE(episode_id, idx)
);

CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    step_id TEXT NOT NULL,
    episode_id TEXT NOT NULL,
    tool_name TEXT,
    call_index INTEGER, -- Added for uniqueness if multiple tool calls per step
    args TEXT,
    args_sha256 TEXT,
    result TEXT,
    result_sha256 TEXT,
    error TEXT,
    truncations_json TEXT,
    meta_json TEXT,
    FOREIGN KEY(step_id) REFERENCES steps(id),
    UNIQUE(step_id, call_index)
);

CREATE INDEX IF NOT EXISTS idx_steps_episode ON steps(episode_id, idx);
CREATE INDEX IF NOT EXISTS idx_tool_calls_episode ON tool_calls(episode_id);
CREATE INDEX IF NOT EXISTS idx_tool_calls_step ON tool_calls(step_id);
"#;
