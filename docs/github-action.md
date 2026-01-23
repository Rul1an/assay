# GitHub Actions Example

This example shows how to integrate Assay into your CI/CD pipeline.

## Basic Setup

```yaml
# .github/workflows/eval.yaml
name: Agent Evaluation

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]

jobs:
  eval:
    runs-on: ubuntu-latest

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install Assay
        run: cargo install assay-cli

      - name: Run evaluations
        run: |
          assay run \
            --config mcp-eval.yaml \
            --trace-file traces/golden.jsonl \
            --strict
```

## With Caching (Faster Builds)

```yaml
# .github/workflows/eval.yaml
name: Agent Evaluation

on:
  pull_request:
    branches: [main]

jobs:
  eval:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
          key: ${{ runner.os }}-cargo-assay-${{ hashFiles('**/Cargo.lock') }}

      - name: Install Assay
        run: |
          if ! command -v assay &> /dev/null; then
            cargo install assay-cli
          fi

      - name: Run evaluations
        run: |
          assay run \
            --config mcp-eval.yaml \
            --trace-file traces/golden.jsonl \
            --strict \
            --db :memory:
```

## Multiple Test Suites

```yaml
# .github/workflows/eval.yaml
name: Agent Evaluation

on:
  pull_request:
    branches: [main]

jobs:
  eval:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        suite:
          - name: core
            config: evals/core.yaml
            trace: traces/core.jsonl
          - name: security
            config: evals/security.yaml
            trace: traces/security.jsonl
          - name: edge-cases
            config: evals/edge-cases.yaml
            trace: traces/edge-cases.jsonl

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install Assay
        run: cargo install assay-cli

      - name: Run ${{ matrix.suite.name }} evaluations
        run: |
          assay run \
            --config ${{ matrix.suite.config }} \
            --trace-file ${{ matrix.suite.trace }} \
            --strict \
            --db :memory:
```

## With Artifact Upload

```yaml
# .github/workflows/eval.yaml
name: Agent Evaluation

on:
  pull_request:
    branches: [main]

jobs:
  eval:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install Assay
        run: cargo install assay-cli

      - name: Run evaluations
        id: eval
        run: |
          assay run \
            --config mcp-eval.yaml \
            --trace-file traces/golden.jsonl \
            --strict \
            --db .assay/eval.db
        continue-on-error: true

      - name: Upload results
        uses: actions/upload-artifact@v4
        if: always()
        with:
          name: assay-results
          path: |
            .assay/
            run.json
          retention-days: 7

      - name: Check result
        if: steps.eval.outcome == 'failure'
        run: exit 1
```

## Monorepo Setup

```yaml
# .github/workflows/eval.yaml
name: Agent Evaluation

on:
  pull_request:
    paths:
      - 'agents/**'
      - 'evals/**'

jobs:
  eval:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install Assay
        run: cargo install assay-cli

      - name: Run evaluations
        working-directory: ./agents/my-agent
        run: |
          assay run \
            --config ../../evals/my-agent.yaml \
            --trace-file ../../traces/my-agent.jsonl \
            --strict
```

## Required Files

Your repository should include:

```
your-repo/
├── .github/
│   └── workflows/
│       └── eval.yaml          # This workflow
├── mcp-eval.yaml              # Your eval config
├── traces/
│   └── golden.jsonl           # Golden trace file(s)
└── ...
```

## Tips

### Use In-Memory Database in CI

```bash
assay run --db :memory:
```

This avoids permission issues and ensures clean runs.

### Fail Fast with --strict

```bash
assay run --strict
```

Returns exit code 1 on any failure, which fails the GitHub Action.

### Debug Failures

Add debug logging for troubleshooting:

```yaml
- name: Run evaluations (debug)
  run: |
    RUST_LOG=assay=debug assay run \
      --config mcp-eval.yaml \
      --trace-file traces/golden.jsonl
```

### Cache the Assay Binary

The Cargo cache action above caches the compiled binary, making subsequent runs faster.
