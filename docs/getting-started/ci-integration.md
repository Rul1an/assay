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
# .github/workflows/assay.yml
name: AI Agent Security

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read
  security-events: write
  pull-requests: write

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0

      - name: Run tests with Assay
        run: |
          curl -fsSL https://getassay.dev/install.sh | sh
          assay ci --config ci-eval.yaml --trace-file traces/ci.jsonl --sarif .assay/reports/sarif.json --junit .assay/reports/junit.xml

      - name: Verify AI agent behavior
        uses: Rul1an/assay-action@v3
        with:
          fail_on: error
```

Canonical public slug: `Rul1an/assay-action@v3` (Marketplace).
This repository's workflows execute the commit in `.github/assay-action-pin`;
`./assay-action` is not a substitute.

### Action Inputs

| Input | Description | Default |
|-------|-------------|---------|
| `bundles` | Glob pattern for evidence bundles | Auto-detect |
| `fail_on` | Fail threshold: `error`, `warn`, `info`, `none` | `error` |
| `sarif` | Upload to GitHub Security tab | `true` |
| `comment_diff` | Post PR comment (only if findings) | `true` |
| `baseline_key` | Key for baseline comparison | - |
| `write_baseline` | Save baseline (main branch only) | `false` |

### Action Outputs

| Output | Description |
|--------|-------------|
| `verified` | `true` if all bundles verified |
| `findings_error` | Count of error-level findings |
| `findings_warn` | Count of warning-level findings |

### SARIF Integration (Automatic)

The action automatically uploads SARIF results to GitHub Code Scanning. Findings appear in the Security tab and inline in PR diffs.

No manual SARIF upload step needed - just add `security-events: write` permission.

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
    - cargo install assay-cli --version 6.0.0 --locked
  script:
    - assay ci --config eval.yaml --trace-file traces/golden.jsonl --junit .assay/reports/junit.xml
    - assay ci --config eval.yaml --trace-file traces/golden.jsonl --sarif .assay/reports/sarif.json
  artifacts:
    reports:
      junit: .assay/reports/junit.xml
    when: always
```

### GitLab Security Report (SARIF)

```yaml
assay:
  script:
    - assay ci --config eval.yaml --trace-file traces/golden.jsonl --sarif .assay/reports/sarif.json
  artifacts:
    paths:
      - .assay/reports/sarif.json
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
  - script: cargo install assay-cli --version 6.0.0 --locked
    displayName: 'Install Assay'

  - script: assay ci --config eval.yaml --trace-file traces/golden.jsonl --strict --junit .assay/reports/junit.xml
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
          command: cargo install assay-cli --version 6.0.0 --locked
      - run:
          name: Run Tests
          command: assay ci --config eval.yaml --trace-file traces/golden.jsonl --strict --junit .assay/reports/junit.xml
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
                sh 'cargo install assay-cli --version 6.0.0 --locked'
            }
        }

        stage('Run Tests') {
            steps {
                sh 'assay ci --config eval.yaml --trace-file traces/golden.jsonl --junit .assay/reports/junit.xml'
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

For environments without a preinstalled Rust toolchain, download and verify the explicit `v6.0.0` release asset during a trusted setup stage, then cache that exact binary. Assay does not currently claim a verified public GHCR image.

---

## Best Practices

### 1. Store Golden Traces in Git

```
your-repo/
├── eval.yaml
├── policies/
│   └── discount.yaml
└── traces/
    └── golden.jsonl  # ← Commit this
```

### 2. Use `fail_on` for Strict Mode

```yaml
- uses: Rul1an/assay-action@v3
  with:
    fail_on: warn  # Fail on warnings AND errors
```

### 3. Cache Cargo Installation

```yaml
- uses: actions/cache@caa296126883cff596d87d8935842f9db880ef25 # v5.1.0
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
      - 'eval.yaml'
```

### 5. Separate Fast and Slow Tests

```yaml
jobs:
  assay:
    # Evidence verification (fast)
    steps:
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
      - uses: Rul1an/assay-action@v3

  integration:
    needs: assay
    # Real LLM tests (slow) — only if Assay passes
    steps:
      - run: pytest tests/integration
```

---

## Debugging CI Failures

### View Detailed Output

```yaml
- run: assay doctor --config eval.yaml --trace-file traces/golden.jsonl
```

### Download Artifacts

```yaml
- uses: actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1
  with:
    name: assay-reports
    path: .assay/reports/
```

### Local Reproduction

```bash
# Same command as CI
assay ci --config eval.yaml --trace-file traces/golden.jsonl --strict --db :memory: --sarif .assay/reports/sarif.json --junit .assay/reports/junit.xml
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

- [:octicons-arrow-right-24: Write custom policies](../reference/config/policies.md)
- [:octicons-arrow-right-24: Debugging failed tests](../use-cases/debugging.md)
- [:octicons-arrow-right-24: Sequence validation](../reference/config/sequences.md)
