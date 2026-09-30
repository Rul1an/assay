# CI Integration

Add Assay to your CI/CD pipeline to evaluate recorded agent traces.

---

## Why CI Integration?

Illustrative live-test flow (the duration below is not a measurement):

```
PR opened → Run LLM tests → Wait 3 minutes → Random failure → Retry → Trust erodes
```

Replay flow with Assay:

```
PR opened → Replay recorded traces → Evaluate configured checks → Report results
```

Replay reuses recorded outputs. Repeatable pass/fail results require the same trace,
configuration and deterministic evaluators, with the Assay version and any evaluator
versions, seeds and dependencies held fixed. Live or nondeterministic evaluators need
their own controls. Replay does not prevent failures in downloads, the runner or other
CI infrastructure.

---

## GitHub Actions

### Using the Assay Action (Recommended)

This example assumes the checkout provides `ci-eval.yaml`, `traces/ci.jsonl`, and an existing evidence bundle (`*.tar.gz`) under `evidence/` or `.assay/evidence/`. If another job or system produces the bundle, add a step to retrieve it before the Action runs. The `assay ci` command below produces SARIF and JUnit reports; it does not create the bundle consumed by the Action. Without a discovered bundle, this example does not establish bundle verification.

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

### SARIF Upload Conditions

With `sarif: true`, the Action attempts to upload its discovered-bundle lint report (`.assay-reports/lint.sarif`) when a bundle is found. It skips this upload for fork pull requests. The job needs `security-events: write` and repository access to Code Scanning; permissions alone do not establish a successful upload or a displayed finding. Check the upload result and the repository UI separately.

The CLI report created above at `.assay/reports/sarif.json` is a different file. The Action does not automatically upload that CLI output; configure a separate SARIF upload step if you want to publish it.

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
    - cargo install assay-cli --version 6.9.0 --locked
  script:
    - assay ci --config eval.yaml --trace-file traces/golden.jsonl --junit .assay/reports/junit.xml
    - assay ci --config eval.yaml --trace-file traces/golden.jsonl --sarif .assay/reports/sarif.json
  artifacts:
    reports:
      junit: .assay/reports/junit.xml
    when: always
```

### Retain a SARIF Artifact in GitLab

This job saves the SARIF file as a downloadable artifact. It does not configure GitLab security-report ingestion or establish that findings appear in a security dashboard.

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
  - script: cargo install assay-cli --version 6.9.0 --locked
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
          command: cargo install assay-cli --version 6.9.0 --locked
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
                sh 'cargo install assay-cli --version 6.9.0 --locked'
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

For environments without a preinstalled Rust toolchain, download and verify the explicit `v6.9.0` release asset during a trusted setup stage, then cache that exact binary. The verified container image for `assay-mcp-server` is documented in the [installation guide](installation.md#container-image-assay-mcp-server).

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

The following is a jobs fragment for an existing workflow, not a complete workflow. Supply its triggers, permissions, evidence bundles, and the Python/dependency/auth setup required by your integration tests. The `needs` relationship sequences jobs; it does not guarantee their relative duration.

```yaml
jobs:
  assay:
    runs-on: ubuntu-latest
    # Evidence verification
    steps:
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
      - uses: Rul1an/assay-action@v3

  integration:
    needs: assay
    runs-on: ubuntu-latest
    # Live integration tests — only after the assay job succeeds
    steps:
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
      # Add your Python and project-dependency setup here.
      - run: pytest tests/integration
```

---

## Debugging CI Failures

### View Detailed Output

```yaml
- run: assay doctor --config eval.yaml --trace-file traces/golden.jsonl
```

### Upload Reports as Artifacts

Add this step after report generation to upload `.assay/reports/` as a workflow artifact. After a successful upload, download the artifact from the workflow run.

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

Measure performance on your own workload and CI runner. Record the Assay version,
trace size, test count, evaluator configuration, runner resources and cache state.
Measure installation, evaluation and total job duration separately, including both
cold-cache and warm-cache runs when caching is part of your workflow.

If you compare replay with live model tests, report the model/provider, workload,
request volume and dated pricing basis alongside observed duration and cost. Recorded
outputs avoid generating those outputs again; evaluator or integration steps may
still make external calls. Include those calls and runner usage in the comparison.

---

## Next Steps

- [:octicons-arrow-right-24: Write custom policies](../reference/config/policies.md)
- [:octicons-arrow-right-24: Debugging failed tests](../use-cases/debugging.md)
- [:octicons-arrow-right-24: Sequence validation](../reference/config/sequences.md)
