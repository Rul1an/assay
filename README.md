# Assay

[![Crates.io](https://img.shields.io/crates/v/assay-cli.svg)](https://crates.io/crates/assay-cli)
[![CI](https://github.com/Rul1an/assay/actions/workflows/ci.yml/badge.svg)](https://github.com/Rul1an/assay/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/assay-core.svg)](https://github.com/Rul1an/assay/blob/main/LICENSE)

Policy-as-Code for AI agents.

Deterministic MCP governance, CI gates, and verifiable evidence bundles.
Runs offline-first with no required hosted backend.

Assay validates tool-call behavior against explicit policy, records auditable decisions, and produces replayable evidence. It is built for teams that want hard gates and reviewable artifacts.

## Why Assay

- Deterministic gates for MCP-compatible agents in local runs and CI
- Auditable evidence with export, verify, lint, diff, and replay flows
- Runtime control on the tool-call path via `assay mcp wrap`
- Offline-first workflow with portable outputs
- DX-first CLI with SARIF, JUnit, PR-comment, and markdown outputs

## Security Model (Bounded Claims)

Assay’s strongest wedge is deterministic governance on the tool-call route.

In the MCP fragmented-IPI experiment line, stateful sequence policy remained effective across payload fragmentation, tool-hopping, sink-failure pressure, and delayed cross-session sink attempts, where wrap-only lexical checks failed.

Assay does not claim to solve semantic hijacking in general, and it does not claim to block raw outbound network bytes by itself. The bounded claim is narrower: Assay governs sink-call routes with explicit policy decisions, audit-grade evidence, and low single-digit millisecond overhead in the published experiment line.

Results and rerun docs:

- [Fragmented IPI results](docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-2026Q1-RESULTS.md)
- [Wrap-bypass results](docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-WRAP-BYPASS-2026Q1-RESULTS.md)
- [Second-sink results](docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SECOND-SINK-2026Q1-RESULTS.md)
- [Cross-session decay results](docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-CROSS-SESSION-DECAY-2026Q1-RESULTS.md)
- [Sink-failure results](docs/ops/EXPERIMENT-MCP-FRAGMENTED-IPI-SINK-FAILURE-2026Q1-RESULTS.md)

## Open Core Boundary

Open core covers the engine, CLI, runtime governance, evidence flows, and baseline packs.

Compliance packs and organization-specific governance packs can be commercial. See [ADR-016](docs/architecture/ADR-016-Pack-Taxonomy.md).

## Quickstart

### Install

```bash
cargo install assay-cli
```

### From scratch

```bash
# Scaffold config + policy + CI
assay init --ci

# Run an offline smoke gate
assay ci --config eval.yaml --trace-file traces/hello.jsonl
```

### From an existing trace

```bash
# Generate policy from recorded behavior
assay init --from-trace trace.jsonl

# Validate trace against config + policy
assay validate --config eval.yaml --trace-file trace.jsonl
```

### From an MCP Inspector session

```bash
# Import Inspector session to Assay trace format
assay import --format inspector session.json --out-trace traces/session.jsonl

# Run policy checks
assay run --config eval.yaml --trace-file traces/session.jsonl
```

### Demo

```bash
make demo   # full break/fix walkthrough
make test   # safe trace (PASS)
make fail   # unsafe trace (FAIL)
```

## Core Commands

### Testing and validation

| Command | What it does |
| --- | --- |
| `assay run` | Execute a test suite against a trace and write run outputs. |
| `assay ci` | CI-mode run with SARIF, JUnit, and PR-comment outputs. |
| `assay validate` | Stateless policy validation with text, JSON, or SARIF output. |
| `assay replay` | Replay from a self-contained offline bundle. |

### Policy and config

| Command | What it does |
| --- | --- |
| `assay init` | Scaffold policy, config, and CI workflow. |
| `assay generate` | Generate policy from traces or profiles. |
| `assay profile` | Multi-run stability profiling. |
| `assay doctor` | Diagnose config, trace, baseline, and runtime issues. |
| `assay explain` | Explain policy behavior against a trace. |

### Evidence and compliance

| Command | What it does |
| --- | --- |
| `assay evidence export` | Create an evidence bundle. |
| `assay evidence verify` | Verify bundle integrity. |
| `assay evidence lint` | Lint evidence with optional packs and SARIF output. |
| `assay evidence diff` | Diff two verified bundles. |
| `assay evidence push/pull/list` | BYOS object storage flows. |

### Runtime

| Command | What it does |
| --- | --- |
| `assay mcp wrap` | Wrap an MCP process with policy enforcement. |
| `assay sandbox` | Rootless Landlock sandbox execution on Linux. |
| `assay monitor` | eBPF/LSM runtime enforcement on Linux. |

### Misc

| Command | What it does |
| --- | --- |
| `assay import` | Import traces from Inspector or JSON-RPC logs. |
| `assay tool sign/verify/keygen` | Local-key tool signing and verification. |
| `assay fix` | Interactive policy fix suggestions. |

## CI Integration

### GitHub Actions

```yaml
name: Assay Gate
on: [push, pull_request]

permissions:
  contents: read
  pull-requests: write
  security-events: write

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@<PINNED_SHA>
      - uses: Rul1an/assay-action@v2
```

Assay Action installs Assay, runs the gate, uploads SARIF, and can publish PR-friendly outputs.

You can also generate a starter workflow:

```bash
assay init --ci github
assay init --ci gitlab
```

Or run manually:

```bash
assay ci \
  --config eval.yaml \
  --trace-file traces/golden.jsonl \
  --sarif reports/sarif.json \
  --junit reports/junit.xml \
  --pr-comment reports/pr-comment.md \
  --replay-strict
```

Exit codes:

- `0` pass
- `1` test failure
- `2` config or measurement error
- `3` infra error

## Configuration

Assay usually works with two files:

- `eval.yaml` for the test suite
- `policy.yaml` for the allowed behavior

`eval.yaml`:

```yaml
version: 1
suite: "my_agent"
model: "trace"
tests:
  - id: "deploy_args"
    input:
      prompt: "deploy_staging"
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
          required: [env]
          properties:
            env:
              type: string
              enum: [staging, prod]
```

`policy.yaml`:

```yaml
version: "1.0"
name: "my-policy"
allow: ["*"]
deny:
  - "exec"
  - "shell"
  - "bash"
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*|^/data/.*"
```

Starter presets:

```bash
assay init --preset default
assay init --preset hardened
assay init --preset dev
```

## Evidence Bundles

Assay produces tamper-evident `.tar.gz` bundles with manifests, hashes, and event streams.

```bash
assay evidence export --profile profile.yaml --out bundle.tar.gz
assay evidence verify bundle.tar.gz
assay evidence lint --pack eu-ai-act-baseline bundle.tar.gz
assay evidence diff baseline.tar.gz current.tar.gz
```

## Python Package

The Python package is published as `assay-it`:

```bash
pip install assay-it
```

## Standards and Related Projects

Assay is easier to evaluate when mapped to established specs and ecosystems:

- [Model Context Protocol (MCP)](https://github.com/modelcontextprotocol/modelcontextprotocol)
- [OpenTelemetry specification](https://github.com/open-telemetry/opentelemetry-specification)
- [CloudEvents specification](https://github.com/cloudevents/spec)
- [SARIF specification](https://github.com/oasis-tcs/sarif-spec)
- [JSON Schema specification](https://github.com/json-schema-org/json-schema-spec)

These are interoperability references, not claims of full feature parity with each project.

## Documentation

- Getting started: [`docs/getting-started/quickstart.md`](docs/getting-started/quickstart.md)
- CI guide: [`docs/guides/github-action.md`](docs/guides/github-action.md)
- MCP quickstart: [`docs/mcp/quickstart.md`](docs/mcp/quickstart.md)
- Use cases: [`docs/use-cases/index.md`](docs/use-cases/index.md)
- Experiment runbooks/results: [`docs/ops/`](docs/ops/)
- Architecture index: [`docs/architecture/index.md`](docs/architecture/index.md)
- ADR index: [`docs/architecture/adrs.md`](docs/architecture/adrs.md)
- Roadmap: [`docs/ROADMAP.md`](docs/ROADMAP.md)
- Contributing docs: [`docs/contributing/index.md`](docs/contributing/index.md)

## Contributing

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

See [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

[MIT](LICENSE)
