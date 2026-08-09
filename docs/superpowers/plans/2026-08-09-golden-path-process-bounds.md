# Golden Path Process Bounds Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the golden-path execution gate prove post-execution observation, reject unknown binaries, and bound every CLI/Python subprocess it owns.

**Architecture:** Consolidate contract lookup and observation accounting in the existing workspace-only support module, and add one standard-library bounded process helper under `tests/support/` for both crate-local integration tests. The helper pipes stdin/stdout/stderr, enforces a wall-clock deadline and independent byte ceilings, kills and reaps on timeout or overflow, and returns diagnostics containing the logical context plus the spawned command.

**Tech Stack:** Rust standard library process/thread/channel APIs, `serde_json`, Cargo integration tests, Markdown plans.

## Global Constraints

- Base all work on `48beb09d1ba581f275da7775228262c778914a28` in `/Users/roelschuurkes/.config/superpowers/worktrees/assay-2173-process-bounds`.
- Use `CARGO_TARGET_DIR=/tmp/assay-2173-process-bounds-target`; do not share another worktree's target directory.
- No production CLI or MCP behavior changes.
- Generated contract reads remain ordinary generator-owned repository reads; do not add a hostile-input ceiling to them.
- Preserve standalone scenario tests and orchestrator execution.
- Use one implementation for the process rule and one implementation for contract outcome lookup.
- Bound stdout and stderr independently at 1 MiB and use a 10-second default deadline.
- On timeout or overflow, kill and reap before returning an error.
- Keep command context and bounded stdout/stderr excerpts in failure diagnostics.
- Do not add a Python version pin.

---

### Task 1: Post-observation coverage and closed binary set

**Files:**
- Modify: `tests/support/agent_golden_path.rs`
- Modify: `crates/assay-cli/tests/agent_golden_path_contract.rs`
- Modify: `crates/assay-mcp-server/tests/agent_golden_path_contract.rs`

**Interfaces:**
- Produces: `ExpectedOutcome`, `expected_outcome(&Value, &str, &str)`, `record_observation(&ExpectedOutcome)`, and `assert_contract_binaries(&Value, &[&str])` in shared test support.
- Consumes: the canonical generated contract and existing thread-local exact-coverage map.

- [ ] **Step 1: Write failing support tests**

Add a test proving lookup alone does not satisfy `assert_exact`:

```rust
fn lookup_only_scenario() {
    let contract = fixture_contract();
    let _ = expected_outcome(&contract, "doctor", "success");
}

#[test]
fn lookup_without_observation_fails_exact_coverage() {
    let panic = std::panic::catch_unwind(|| {
        assert_exact(&fixture_contract(), "assay", &[lookup_only_scenario]);
    });
    assert!(panic.is_err(), "lookup alone unexpectedly counted as observation");
}
```

Add a binary-set test whose third-binary mutation fails:

```rust
#[test]
fn contract_binary_set_is_closed() {
    assert_contract_binaries(
        &fixture_contract(),
        &["assay", "assay-mcp-server"],
    );
    let mut mutated = fixture_contract();
    mutated["steps"].as_array_mut().unwrap().push(json!({
        "id": "third", "binary": "other", "outcomes": []
    }));
    assert!(std::panic::catch_unwind(|| {
        assert_contract_binaries(&mutated, &["assay", "assay-mcp-server"]);
    }).is_err());
}
```

- [ ] **Step 2: Run the focused tests and verify RED**

Run:

```bash
CARGO_TARGET_DIR=/tmp/assay-2173-process-bounds-target \
  cargo test -p assay-cli --test agent_golden_path_contract --locked -- --nocapture
```

Expected: compile failure naming missing `ExpectedOutcome`, `expected_outcome`, or `assert_contract_binaries`.

- [ ] **Step 3: Implement shared lookup and observation**

Move duplicated lookup logic into `tests/support/agent_golden_path.rs`:

```rust
pub struct ExpectedOutcome {
    step_id: String,
    outcome_name: String,
    value: Value,
}

impl std::ops::Deref for ExpectedOutcome {
    type Target = Value;
    fn deref(&self) -> &Self::Target { &self.value }
}

pub fn expected_outcome(
    contract: &Value,
    step_id: &str,
    outcome_name: &str,
) -> ExpectedOutcome {
    // Locate the step/outcome once, clone it, and attach command, binary, and
    // working_directory from the step. Do not record coverage here.
}

pub fn record_observation(outcome: &ExpectedOutcome) {
    record_outcome(&outcome.step_id, &outcome.outcome_name);
}
```

Implement `assert_contract_binaries` by collecting all step `binary` strings
into a `BTreeSet` and comparing them with the expected set. Replace both local
`expected_outcome` functions with calls to this shared implementation.

- [ ] **Step 4: Move registration to observed-result sites**

Change both `assert_exit` helpers to accept `&ExpectedOutcome` and call
`record_observation(expected)` only after the child status has been read and
matched. For the JSON-RPC policy-denial path, call `record_observation` only
after `connection.shutdown()` returns and its status matches. Add one real
contract test calling `assert_contract_binaries` against exactly `assay` and
`assay-mcp-server`.

- [ ] **Step 5: Run both focused suites and verify GREEN**

```bash
CARGO_TARGET_DIR=/tmp/assay-2173-process-bounds-target \
  cargo test -p assay-cli --test agent_golden_path_contract --locked -- --nocapture
CARGO_TARGET_DIR=/tmp/assay-2173-process-bounds-target \
  cargo test -p assay-mcp-server --test agent_golden_path_contract --locked -- --nocapture
```

- [ ] **Step 6: Commit Task 1**

```bash
git add -A tests/support/agent_golden_path.rs \
  crates/assay-cli/tests/agent_golden_path_contract.rs \
  crates/assay-mcp-server/tests/agent_golden_path_contract.rs
git commit -m "test(agent): count observed golden-path outcomes"
```

### Task 2: Shared bounded process runner

**Files:**
- Create: `tests/support/bounded_process.rs`
- Modify: `crates/assay-cli/tests/agent_golden_path_contract.rs`

**Interfaces:**
- Produces: `ProcessLimits`, `GOLDEN_PATH_LIMITS`, and `run_bounded(&mut Command, &[u8], ProcessLimits, &str) -> Result<Output, String>`.
- Guarantees: independent stdout/stderr ceilings, deadline, kill/reap, bounded diagnostics, and no surviving child on error.

- [ ] **Step 1: Write timeout and flood tests before implementation**

Include the shared module from the CLI integration test and add Unix/Windows
shell-command factories. Require a timeout and a stdout flood to return errors
that name the supplied context and failure class:

```rust
#[test]
fn bounded_runner_kills_timeout_and_reports_context() {
    let mut command = hanging_command();
    let limits = ProcessLimits::new(Duration::from_millis(100), 1024, 1024);
    let error = run_bounded(&mut command, b"", limits, "hanging mutation")
        .expect_err("hanging child must time out");
    assert!(error.contains("hanging mutation"));
    assert!(error.contains("deadline"));
}

#[test]
fn bounded_runner_kills_output_flood() {
    let mut command = stdout_flood_command();
    let limits = ProcessLimits::new(Duration::from_secs(2), 1024, 1024);
    let error = run_bounded(&mut command, b"", limits, "flood mutation")
        .expect_err("output flood must exceed its ceiling");
    assert!(error.contains("stdout"));
    assert!(error.contains("1024"));
}
```

- [ ] **Step 2: Run the CLI suite and verify RED**

Run the Task 1 CLI command. Expected: compile failure because
`tests/support/bounded_process.rs` or its interfaces do not exist.

- [ ] **Step 3: Implement `run_bounded`**

Use only standard-library APIs:

```rust
#[derive(Clone, Copy)]
pub struct ProcessLimits {
    pub timeout: Duration,
    pub max_stdout_bytes: usize,
    pub max_stderr_bytes: usize,
}

pub const GOLDEN_PATH_LIMITS: ProcessLimits = ProcessLimits {
    timeout: Duration::from_secs(10),
    max_stdout_bytes: 1024 * 1024,
    max_stderr_bytes: 1024 * 1024,
};
```

`run_bounded` must:

1. force piped stdin/stdout/stderr and retain `format!("{command:?}")`;
2. spawn one stdin writer and one bounded reader per output stream;
3. have readers signal overflow after `limit + 1` bytes;
4. poll `try_wait()` until exit, overflow, or deadline;
5. kill and `wait()` on overflow/deadline before joining threads;
6. return `Output` only when both streams stay within their limits;
7. include context, command, status/reap result, stream, ceiling, and bounded
   stdout/stderr excerpts in errors.

- [ ] **Step 4: Run bounded-runner tests and verify GREEN**

Run the Task 1 CLI command and require both new negative controls to pass on
the current platform.

- [ ] **Step 5: Commit Task 2**

```bash
git add -A tests/support/bounded_process.rs \
  crates/assay-cli/tests/agent_golden_path_contract.rs
git commit -m "test(agent): add bounded process runner"
```

### Task 3: Apply the one process rule to all owned paths

**Files:**
- Modify: `crates/assay-cli/tests/agent_golden_path_contract.rs`
- Modify: `crates/assay-mcp-server/tests/agent_golden_path_contract.rs`
- Modify: `docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md`

**Interfaces:**
- Consumes: `run_bounded` and `GOLDEN_PATH_LIMITS` from Task 2.
- Removes: the MCP-local `wait_bounded`, `PROCESS_TIMEOUT`, and `MAX_STDOUT_BYTES` implementation.

- [ ] **Step 1: Write integration assertions for bounded diagnostics**

Require the CLI `assay()` helper and MCP `run_server()` helper to return
`run_bounded(...).unwrap_or_else(|error| panic!("{error}"))`. Change the Python
availability probe to use the same helper with context
`"protected-action Python preflight"`.

- [ ] **Step 2: Run both suites and verify RED**

Temporarily make the local MCP `wait_bounded` unavailable. Run both focused
suites and verify the MCP suite fails to compile until it imports the shared
runner.

- [ ] **Step 3: Replace all three owned subprocess paths**

Add the same path import to both integration tests:

```rust
#[path = "../../../tests/support/bounded_process.rs"]
mod bounded_process;
```

Route CLI commands, MCP stdin-driven commands, and `python --version` through
`run_bounded`. Preserve `Conn` for interactive JSON-RPC; it already has its own
bounded response/shutdown behavior. Delete the duplicate MCP wait function and
its reader/time imports.

- [ ] **Step 4: Correct the historical Task 2 RED statement**

Replace the stale claim that the machine contract is absent with the actual
checkpoint: MCP-owned protected-action and SARIF outcomes are missing or
incomplete at that point. Do not rewrite historical commands or checked boxes.

- [ ] **Step 5: Run both focused suites and verify GREEN**

Run both Task 1 commands. Confirm standalone scenario tests and exact-coverage
orchestrators still execute.

- [ ] **Step 6: Commit Task 3**

```bash
git add -A tests/support/bounded_process.rs \
  crates/assay-cli/tests/agent_golden_path_contract.rs \
  crates/assay-mcp-server/tests/agent_golden_path_contract.rs \
  docs/superpowers/plans/2026-08-08-agent-golden-path-contract.md
git commit -m "test(agent): bound golden-path subprocesses"
```

### Task 4: Mutation proof and final verification

**Files:**
- Modify if required by review only: the four files from Tasks 1-3
- Update: PR body and issue #2173 with exact-SHA evidence after commit

**Interfaces:**
- Produces: exact-head verification and mutation evidence for review.

- [ ] **Step 1: Run the four required mutations one at a time**

Each temporary mutation must fail for its own reason:

1. replace a scenario body with `expected_outcome(...)` only: exact coverage
   reports the missing outcome;
2. append a third `binary` to a scratch contract: binary-set assertion reports
   `other`;
3. run the bounded helper against the hanging command: deadline diagnostic and
   reaped child;
4. run it against the flood command: named stream/ceiling diagnostic and reaped
   child.

Restore every mutation and verify `git diff` contains only intended changes.

- [ ] **Step 2: Run affected verification**

```bash
export CARGO_TARGET_DIR=/tmp/assay-2173-process-bounds-target
cargo test -p assay-cli --test agent_golden_path_contract --locked -- --nocapture
cargo test -p assay-mcp-server --test agent_golden_path_contract --locked -- --nocapture
cargo test -p assay-cli --locked
cargo test -p assay-mcp-server --locked
cargo fmt --all -- --check
cargo clippy -p assay-cli -p assay-mcp-server --all-targets --all-features --locked -- -D warnings
python3 scripts/ci/test-agent-golden-path-skill.py
scripts/ci/check-docs-generated-drift.sh
git diff --check
```

- [ ] **Step 3: Run the shared simulation/eval matrix**

```bash
cargo test -p assay-sim --locked
cargo test -p assay-core agentic::tests --locked
cargo test -p assay-core judge_internal::tests::contract --locked
```

Record executed/ignored counts as measurements, not coverage claims.

- [ ] **Step 4: Commit any verification-only correction**

Stage only named changed paths. Do not create an empty commit.

- [ ] **Step 5: Push, open PR, and request exact-head reviews**

Open a ready PR with `Closes #2173`, exact head/worktree/toolchain provenance,
RED/GREEN evidence, mutation results, verification commands, and non-claims.
Request Claude Code Desktop read-only review plus CodeRabbit or Copilot. A new
push invalidates prior reviews.
