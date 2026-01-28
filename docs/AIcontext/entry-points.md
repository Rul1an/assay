# Entry Points

This document catalogs all ways to interact with Assay: CLI commands, Python SDK methods, MCP server endpoints, and configuration files.

## CLI Commands

All CLI commands are defined in `crates/assay-cli/src/cli/args.rs` and dispatched in `crates/assay-cli/src/cli/commands/mod.rs`.

### Core Commands

#### `assay run`
**Purpose**: Execute test suite against traces
**Entry**: `crates/assay-cli/src/cli/commands/mod.rs::cmd_run()`
**Flow**: `load_config()` → `build_runner()` → `Runner::run_suite()` → report

**Key Options**:
- `--config <PATH>`: Config file (default: `assay.yaml`)
- `--trace-file <PATH>`: Trace file to use
- `--baseline <PATH>`: Baseline file for regression testing
- `--export-baseline <PATH>`: Export baseline after run
- `--format <FORMAT>`: Output format (console, json, junit, sarif)
- `--strict`: Fail on any violation
- `--rerun-failures <N>`: Retry failed tests N times

#### `assay validate`
**Purpose**: Stateless validation of traces against policy
**Entry**: `crates/assay-cli/src/cli/commands/validate.rs::run()`
**Flow**: `load_config()` → `validate::validate()` → report

**Key Options**:
- `--config <PATH>`: Policy config file
- `--trace-file <PATH>`: Trace file to validate
- `--format <FORMAT>`: Output format (text, json, sarif)

#### `assay init`
**Purpose**: Initialize new Assay project
**Entry**: `crates/assay-cli/src/cli/commands/init.rs::run()`
**Flow**: Detect project type → generate `assay.yaml` + `policy.yaml`

**Key Options**:
- `--force`: Overwrite existing config
- `--template <TEMPLATE>`: Use specific template

### Trace Management

#### `assay import`
**Purpose**: Import traces from external formats
**Entry**: `crates/assay-cli/src/cli/commands/import.rs::cmd_import()`
**Flow**: Parse input format → convert to JSONL → optionally generate config

**Supported Formats**:
- `mcp-inspector`: MCP Inspector session logs
- `jsonl`: Direct JSONL import
- `otel`: OpenTelemetry traces

**Key Options**:
- `--format <FORMAT>`: Input format
- `--init`: Auto-generate config
- `--out-trace <PATH>`: Output trace file

#### `assay trace`
**Purpose**: Generate traces from running agent
**Entry**: `crates/assay-cli/src/cli/commands/trace.rs::cmd_trace()`
**Flow**: Wrap agent execution → capture tool calls → write JSONL

#### `assay replay`
**Purpose**: Interactive trace replay
**Entry**: `crates/assay-cli/src/cli/commands/replay.rs` (if exists)
**Flow**: Load trace → step through → inspect results

### Policy Management

#### `assay generate`
**Purpose**: Generate policy from traces (learning mode)
**Entry**: `crates/assay-cli/src/cli/commands/generate.rs::run()`
**Flow**: Analyze traces → generate policy constraints → write `policy.yaml`

**Key Options**:
- `--from-profile <PATH>`: Generate from profile
- `--from-trace <PATH>`: Generate from trace file
- `--output <PATH>`: Output policy file

#### `assay record`
**Purpose**: Capture and generate in one flow
**Entry**: `crates/assay-cli/src/cli/commands/record.rs::run()`
**Flow**: Capture traces → generate policy → save both

#### `assay migrate`
**Purpose**: Migrate config from old to new format
**Entry**: `crates/assay-cli/src/cli/commands/migrate.rs::cmd_migrate()`
**Flow**: Parse old config → transform → write new config

**Key Options**:
- `--config <PATH>`: Config to migrate
- `--dry-run`: Preview changes without writing

### Analysis & Debugging

#### `assay doctor`
**Purpose**: Diagnose common issues
**Entry**: `crates/assay-cli/src/cli/commands/doctor.rs::run()`
**Flow**: Analyze config + traces → report issues → suggest fixes

#### `assay explain`
**Purpose**: Explain policy violations
**Entry**: `crates/assay-cli/src/cli/commands/explain.rs::run()`
**Flow**: Load trace → find violations → generate human-readable explanation

#### `assay coverage`
**Purpose**: Analyze policy coverage
**Entry**: `crates/assay-cli/src/cli/commands/coverage.rs::cmd_coverage()`
**Flow**: Load traces + policy → calculate coverage → report

**Key Options**:
- `--min-coverage <PERCENT>`: Minimum coverage threshold
- `--trace-file <PATH>`: Trace file to analyze

### Baseline Management

#### `assay baseline`
**Purpose**: Manage baselines for regression testing
**Entry**: `crates/assay-cli/src/cli/commands/baseline.rs`

**Subcommands**:
- `record`: Record baseline from current run
- `check`: Check against baseline
- `report`: Show baseline report

### CI Integration

#### `assay ci`
**Purpose**: CI-optimized test execution
**Entry**: `crates/assay-cli/src/cli/commands/mod.rs::cmd_ci()`
**Flow**: Similar to `run` but optimized for CI (strict mode, SARIF output)

#### `assay init-ci`
**Purpose**: Generate CI workflow files
**Entry**: `crates/assay-cli/src/cli/commands/init_ci.rs::cmd_init_ci()`
**Flow**: Generate GitHub Actions / GitLab CI config

### Runtime Security

#### `assay mcp-server`
**Purpose**: Start Assay as MCP server/proxy
**Entry**: `crates/assay-mcp-server/src/main.rs` (separate binary)
**Flow**: Load policies → start JSON-RPC server → proxy tool calls

**Key Options**:
- `--policy <PATH>`: Policy directory
- `--port <PORT>`: Server port (default: 3000)
- `--host <HOST>`: Server host (default: 127.0.0.1)

#### `assay monitor`
**Purpose**: Runtime eBPF monitoring (Linux only)
**Entry**: `crates/assay-cli/src/cli/commands/monitor.rs::run()`
**Flow**: Load policy → compile Tier 1 rules → load eBPF → monitor process

**Key Options**:
- `--policy <PATH>`: Policy file
- `--pid <PID>`: Process ID to monitor
- `--cgroup <PATH>`: Cgroup to monitor

#### `assay sandbox`
**Purpose**: Secure execution sandbox
**Entry**: `crates/assay-cli/src/cli/commands/sandbox.rs::run()`
**Flow**: Load policy → apply Landlock → execute command

### MCP Management

#### `assay discover`
**Purpose**: Discover MCP servers on machine
**Entry**: `crates/assay-cli/src/cli/commands/discover.rs::run()`
**Flow**: Scan for MCP processes → list servers

#### `assay kill`
**Purpose**: Kill/terminate MCP servers
**Entry**: `crates/assay-cli/src/cli/commands/kill.rs::run()`
**Flow**: Find MCP processes → terminate

### Advanced Features

#### `assay quarantine`
**Purpose**: Manage flaky test quarantine
**Entry**: `crates/assay-cli/src/cli/commands/mod.rs::cmd_quarantine()`
**Flow**: Mark/unmark tests as quarantined

#### `assay calibrate`
**Purpose**: Calibrate metric thresholds
**Entry**: `crates/assay-cli/src/cli/commands/calibrate.rs::cmd_calibrate()`
**Flow**: Analyze historical results → suggest thresholds

#### `assay profile`
**Purpose**: Manage multi-run profiles
**Entry**: `crates/assay-cli/src/cli/commands/profile.rs::run()`
**Flow**: Collect profiles → analyze stability

#### `assay evidence`
**Purpose**: Evidence management (audit/compliance)
**Entry**: `crates/assay-cli/src/cli/commands/evidence/mod.rs::run()`
**Flow**: Export/verify/lint/diff evidence artifacts

**Subcommands**:
- `export`: Export evidence bundle from Profile
- `verify`: Verify bundle integrity and provenance
- `show`: Inspect bundle contents (verify + table view)
- `lint`: Lint bundle for quality and security issues (SARIF output)
- `diff`: Compare two bundles and report changes
- `explore`: Interactive TUI explorer (requires `tui` feature)

**Key Options**:
- `export --profile <PATH>`: Input Profile trace
- `export --out <PATH>`: Output bundle path (.tar.gz)
- `export --detail <LEVEL>`: Detail level (summary, observed, full)
- `verify <BUNDLE>`: Verify bundle (or `-` for stdin)
- `show --no-verify`: Skip verification (show even if corrupt)
- `lint --format sarif`: Output in SARIF format
- `lint --fail-on <SEVERITY>`: Fail on severity threshold
- `diff <BUNDLE1> <BUNDLE2>`: Compare two bundles

#### `assay sim`
**Purpose**: Attack simulation (hardening/compliance)
**Entry**: `crates/assay-cli/src/cli/commands/sim.rs::run()`
**Flow**: Run attack suite → report blocked/bypassed

#### `assay demo`
**Purpose**: Generate demo environments with sample configs
**Entry**: `crates/assay-cli/src/cli/commands/demo.rs::run()`
**Flow**: Create sample project with traces, policies, and configs

#### `assay fix`
**Purpose**: Agentic policy fixing based on violations
**Entry**: `crates/assay-cli/src/cli/commands/fix.rs::run()`
**Flow**: Analyze violations → suggest/apply policy fixes

#### `assay setup`
**Purpose**: Interactive installer and environment setup
**Entry**: `crates/assay-cli/src/cli/commands/setup.rs::run()`
**Flow**: Interactive setup wizard

### Utility Commands

#### `assay version`
**Purpose**: Show version
**Entry**: `crates/assay-cli/src/cli/commands/mod.rs::dispatch()`
**Flow**: Print version string

#### `assay policy`
**Purpose**: Policy management commands
**Entry**: `crates/assay-cli/src/cli/commands/policy.rs::run()`
**Flow**: Various policy operations

## Python SDK Entry Points

Located in `assay-python-sdk/python/assay/`.

### `AssayClient` (`client.py`)

**Purpose**: Record traces to JSONL files

**Key Methods**:
```python
class AssayClient:
    def __init__(self, trace_file: str)
    def record_trace(self, trace: dict) -> None
```

**Usage**:
```python
from assay import AssayClient

client = AssayClient("traces.jsonl")
client.record_trace({
    "tool": "filesystem_read",
    "args": {"path": "/tmp/file.txt"}
})
```

### `Coverage` (`coverage.py`)

**Purpose**: Analyze policy coverage for traces

**Key Methods**:
```python
class Coverage:
    @staticmethod
    def analyze(traces: list, min_coverage: float = 80.0) -> CoverageReport
```

**Usage**:
```python
from assay import Coverage

coverage = Coverage.analyze(traces, min_coverage=80.0)
if not coverage.passed:
    print(f"Coverage: {coverage.score}%")
```

### `Explainer` (`explain.py`)

**Purpose**: Explain policy violations

**Key Methods**:
```python
class Explainer:
    def __init__(self, policy_file: str)
    def explain(self, trace: list) -> str
```

**Usage**:
```python
from assay import Explainer

explainer = Explainer("policy.yaml")
explanation = explainer.explain(trace)
print(explanation)
```

### `validate()` (`__init__.py`)

**Purpose**: Stateless validation function

**Signature**:
```python
def validate(policy_file: str, traces: list) -> dict
```

**Usage**:
```python
from assay import validate

result = validate("policy.yaml", traces)
assert result["passed"]
```

### Pytest Plugin (`pytest_plugin.py`)

**Purpose**: Pytest integration for automatic trace capture

**Fixtures**:
```python
@pytest.fixture
def assay_client() -> AssayClient
```

**Markers**:
```python
@pytest.mark.assay(trace_file="traces.jsonl")
def test_agent():
    pass
```

## GitHub Action

**Repository:** https://github.com/Rul1an/assay-action

### Basic Usage

```yaml
- uses: Rul1an/assay-action@v2
```

### With Options

```yaml
- uses: Rul1an/assay-action@v2
  with:
    bundles: '.assay/evidence/*.tar.gz'
    fail_on: error
    sarif: true
    comment_diff: true
```

### Inputs

| Input | Default | Description |
|-------|---------|-------------|
| `bundles` | Auto-detect | Glob pattern for evidence bundles |
| `fail_on` | `error` | Fail threshold: `error`, `warn`, `info`, `none` |
| `sarif` | `true` | Upload to GitHub Security tab |
| `comment_diff` | `true` | Post PR comment (only if findings) |
| `baseline_key` | - | Key for baseline comparison |
| `write_baseline` | `false` | Save baseline (main branch only) |

### Outputs

| Output | Description |
|--------|-------------|
| `verified` | `true` if all bundles verified |
| `findings_error` | Count of error-level findings |
| `findings_warn` | Count of warning-level findings |
| `reports_dir` | Path to reports directory |

### Permissions Required

```yaml
permissions:
  contents: read
  security-events: write
  pull-requests: write
```

## MCP Server Endpoints

The MCP server (`assay-mcp-server`) exposes tools via JSON-RPC over stdio.

### Tool: `assay_check_args`

**Purpose**: Validate tool arguments before execution

**Request**:
```json
{
  "tool": "assay_check_args",
  "arguments": {
    "target_tool": "apply_discount",
    "args": { "percent": 50 }
  }
}
```

**Response (violation)**:
```json
{
  "allowed": false,
  "violations": [
    {
      "field": "percent",
      "value": 50,
      "constraint": "max: 30",
      "message": "Value exceeds maximum"
    }
  ]
}
```

**Response (valid)**:
```json
{
  "allowed": true,
  "violations": []
}
```

### Tool: `assay_check_sequence`

**Purpose**: Validate tool call sequence

**Request**:
```json
{
  "tool": "assay_check_sequence",
  "arguments": {
    "candidate_tool": "delete_customer",
    "previous_calls": ["get_customer"]
  }
}
```

**Response**: Similar structure to `assay_check_args`

### Tool: `assay_policy_decide`

**Purpose**: General policy decision endpoint

**Request**: Tool call with arguments

**Response**: Allow/deny decision with violations

## Configuration Files

### `assay.yaml`

**Purpose**: Main evaluation configuration
**Location**: Project root (default)
**Schema**: Defined in `assay-core::config`

**Key Sections**:
- `version`: Config version
- `suite`: Suite name
- `model`: LLM model configuration
- `tests`: Test cases
- `settings`: Execution settings

### `policy.yaml`

**Purpose**: Policy constraints
**Location**: Specified in `assay.yaml` or default `policy.yaml`
**Schema**: Defined in `assay-core::policy_engine`

**Key Sections**:
- `tools`: Tool-specific constraints
- `sequences`: Sequence rules
- `blocklist`: Blocked tools/patterns

### Trace Files (`.jsonl`)

**Purpose**: Recorded agent behavior
**Format**: JSON Lines (one JSON object per line)
**Schema**: Defined in `assay-core::trace::schema`

**Example**:
```jsonl
{"tool": "filesystem_read", "args": {"path": "/tmp/file.txt"}}
{"tool": "http_request", "args": {"url": "https://api.example.com"}}
```

## Environment Variables

### `RUST_LOG`
**Purpose**: Control logging level
**Values**: `debug`, `info`, `warn`, `error`
**Default**: `info`

### `MCP_CONFIG_LEGACY`
**Purpose**: Enable legacy config mode
**Values**: `1` to enable
**Default**: Disabled

### `ASSAY_STRICT_DEPRECATIONS`
**Purpose**: Fail on deprecated features
**Values**: `1` to enable
**Default**: Disabled

## Exit Codes

| Code | Meaning | When Used |
|------|---------|-----------|
| 0 | Success | All tests pass |
| 1 | Test failure | One or more tests fail |
| 2 | Config error | Invalid configuration or policy |

## Related Documentation

- [User Flows](user-flows.md) - How these entry points are used in workflows
- [Codebase Overview](codebase-overview.md) - Implementation details
- [Interdependencies](interdependencies.md) - How components connect
