# Assay

**Deterministic testing for AI agents.** Record traces, replay in CI, validate against policies. No API calls, no flakiness.

## 5-Minute Quickstart

### 1. Install

```bash
cargo install assay-cli
```

### 2. Initialize from a trace

If you have an existing trace file (JSONL of agent behavior):

```bash
assay init --from-trace trace.jsonl
```

This generates `policy.yaml` and `eval.yaml` from your trace data.

If you're starting from scratch:

```bash
assay init --ci
```

### 3. Import from MCP Inspector

```bash
assay import --format inspector session.json --init
```

### 4. Run your first test

```bash
assay run --config eval.yaml --trace-file trace.jsonl

# Output:
# Running 3 tests...
# PASS  deploy_args          (0ms)
# PASS  read_file_path       (0ms)
# PASS  no_shell_calls       (0ms)
# Summary: 3 passed, 0 failed, 0 skipped
```

### 5. Add to CI

```bash
assay init --ci github
```

Or use the GitHub Action directly:

```yaml
- uses: Rul1an/assay/assay-action@v2
```

This uploads SARIF to the Security tab and posts a PR comment with results.

## Core Concepts

### Traces

A trace is a recording of agent behavior: tool calls, arguments, responses. Assay replays these deterministically.

```jsonl
{"schema_version": 1, "type": "assay.trace", "request_id": "1", "prompt": "deploy", "response": "{\"status\":\"ok\"}", "model": "trace", "provider": "trace"}
```

### Policies

Policies define allowed behavior:

```yaml
version: "1.0"
name: "my-policy"
allow: ["*"]
deny: ["exec", "shell", "bash"]
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*"
```

### Test Configs

Test configs define what to validate:

```yaml
version: 1
suite: "my_agent"
model: "trace"
tests:
  - id: "basic_flow"
    input:
      prompt: "deploy_staging"
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
          required: [env]
```

### Metrics

Built-in validation types:

| Metric | What it checks |
|--------|---------------|
| `args_valid` | Tool arguments match JSON Schema |
| `sequence_valid` | Tools called in expected order |
| `tool_blocklist` | Forbidden tools never called |
| `regex_match` | Response matches pattern |
| `json_schema` | Response validates against schema |
| `semantic_similarity_to` | Response semantically matches reference |

### Policy Generation

Generate policies from observed behavior instead of writing them by hand:

```bash
# From a single trace
assay generate -i trace.jsonl --heuristics

# From multiple runs (stability analysis)
assay profile init --output profile.yaml --name my-app
assay profile update --profile profile.yaml -i run1.jsonl --run-id run-1
assay profile update --profile profile.yaml -i run2.jsonl --run-id run-2
assay generate --profile profile.yaml --min-stability 0.8
```

### Diagnostics

When things break:

```bash
assay doctor --config eval.yaml --trace-file trace.jsonl
assay explain --trace trace.jsonl --policy policy.yaml
assay fix --config eval.yaml
```

## Documentation

- [CLI Reference: init](reference/cli/init.md)
- [CLI Reference: validate](reference/cli/validate.md)
- [Config Reference](reference/config/index.md)
- [Evidence Bundles](concepts/traces.md)
- [Replay & Caching](concepts/replay.md)
- [GitHub Action Guide](guides/github-action.md)

## License

MIT
