# CI Integration

Add Assay to your CI/CD pipeline for zero-flake AI agent testing.

---

## Why CI Integration?

Traditional approach:

```
PR opened → Run LLM tests → Wait 3 minutes → Random failure → Retry → Trust erodes
```

With Assay:

```
PR opened → Replay traces → 3ms → Deterministic pass/fail → Trust restored
```

---

## GitHub Actions

### Using the Assay Action (Recommended)

```yaml
# .github/workflows/agent-tests.yml
name: Agent Quality Gate

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Run Assay Tests
        uses: Rul1an/assay-action@v1
        with:
          config: mcp-eval.yaml
          trace_file: traces/golden.jsonl
          strict: true
```

### Action Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `config` | Path to `mcp-eval.yaml` | `mcp-eval.yaml` |
| `trace_file` | Path to trace file(s) | Required |
| `strict` | Fail on any violation | `false` |
| `output` | Output format: `sarif`, `junit`, `json` | `sarif` |

### Action Outputs

| Output | Description |
|--------|-------------|
| `passed` | Number of passed tests |
| `failed` | Number of failed tests |
| `exit_code` | 0 if all passed, 1 if any failed |

### SARIF Integration (Code Scanning)

The action uploads SARIF results to GitHub Code Scanning:

```yaml
- uses: Rul1an/assay-action@v1
  with:
    config: mcp-eval.yaml
    output: sarif

- name: Upload SARIF
  uses: github/codeql-action/upload-sarif@v2
  with:
    sarif_file: .assay/reports/results.sarif
```

This shows violations inline in the PR diff:

```
⚠️ args_valid: percent=50 exceeds max(30)
   policies/discount.yaml:8
```

---

## GitLab CI

```yaml
# .gitlab-ci.yml
stages:
  - test

assay:
  stage: test
  image: rust:latest
  before_script:
    - cargo install assay
  script:
    - assay run --config mcp-eval.yaml --trace-file traces/golden.jsonl --output junit
  artifacts:
    reports:
      junit: .assay/reports/junit.xml
    when: always
```

### GitLab Code Quality

```yaml
assay:
  script:
    - assay run --config mcp-eval.yaml --output codeclimate
  artifacts:
    reports:
      codequality: .assay/reports/codeclimate.json
```

---

## Azure Pipelines

```yaml
# azure-pipelines.yml
trigger:
  - main

pool:
  vmImage: 'ubuntu-latest'

steps:
  - script: cargo install assay
    displayName: 'Install Assay'

  - script: assay run --config mcp-eval.yaml --strict --output junit
    displayName: 'Run Assay Tests'

  - task: PublishTestResults@2
    inputs:
      testResultsFormat: 'JUnit'
      testResultsFiles: '.assay/reports/junit.xml'
    condition: always()
```

---

## CircleCI

```yaml
# .circleci/config.yml
version: 2.1

jobs:
  assay:
    docker:
      - image: rust:latest
    steps:
      - checkout
      - run:
          name: Install Assay
          command: cargo install assay
      - run:
          name: Run Tests
          command: assay run --config mcp-eval.yaml --strict
      - store_test_results:
          path: .assay/reports

workflows:
  version: 2
  test:
    jobs:
      - assay
```

---

## Jenkins

```groovy
// Jenkinsfile
pipeline {
    agent any
    
    stages {
        stage('Install Assay') {
            steps {
                sh 'cargo install assay'
            }
        }
        
        stage('Run Tests') {
            steps {
                sh 'assay run --config mcp-eval.yaml --output junit'
            }
        }
    }
    
    post {
        always {
            junit '.assay/reports/junit.xml'
        }
    }
}
```

---

## Docker-Based CI

For environments without Rust:

```yaml
# Any CI system
steps:
  - run: |
      docker run --rm \
        -v $(pwd):/workspace \
        ghcr.io/rul1an/assay:latest \
        run --config /workspace/mcp-eval.yaml --strict
```

---

## Best Practices

### 1. Store Golden Traces in Git

```
your-repo/
├── mcp-eval.yaml
├── policies/
│   └── discount.yaml
└── traces/
    └── golden.jsonl  # ← Commit this
```

### 2. Use `--strict` Mode

```yaml
- uses: Rul1an/assay-action@v1
  with:
    strict: true  # Fail on ANY violation
```

### 3. Cache Cargo Installation

```yaml
- uses: actions/cache@v3
  with:
    path: ~/.cargo
    key: cargo-${{ runner.os }}-assay
```

### 4. Run on Relevant Changes Only

```yaml
on:
  push:
    paths:
      - 'agents/**'
      - 'prompts/**'
      - 'mcp-eval.yaml'
```

### 5. Separate Fast and Slow Tests

```yaml
jobs:
  assay-fast:
    # Deterministic replay (ms)
    steps:
      - uses: Rul1an/assay-action@v1
  
  integration:
    needs: assay-fast
    # Real LLM tests (minutes) — only if fast tests pass
    steps:
      - run: pytest tests/integration
```

---

## Debugging CI Failures

### View Detailed Output

```yaml
- run: assay run --config mcp-eval.yaml --verbose
```

### Download Artifacts

```yaml
- uses: actions/upload-artifact@v3
  with:
    name: assay-reports
    path: .assay/reports/
```

### Local Reproduction

```bash
# Same command as CI
assay run --config mcp-eval.yaml --strict --db :memory:
```

---

## Performance

| Metric | GitHub Actions | GitLab CI |
|--------|----------------|-----------|
| Install time | ~60s (cached: 2s) | ~60s |
| Test time (100 tests) | ~50ms | ~50ms |
| Total job time | ~70s | ~70s |

Compare to LLM-based tests: 3-10 minutes, $0.50-$5.00 per run.

---

## Next Steps

- [:octicons-arrow-right-24: Write custom policies](../config/policies.md)
- [:octicons-arrow-right-24: Debugging failed tests](../use-cases/debugging.md)
- [:octicons-arrow-right-24: Sequence validation](../config/sequences.md)
