# Air-Gapped Enterprise

Run evaluations in secure environments with no external network access.

---

## The Problem

Many organizations cannot use cloud-based AI evaluation tools:

- **Financial services** — PCI-DSS, SOC 2 compliance
- **Healthcare** — HIPAA, patient data protection
- **Government** — FedRAMP, classified environments
- **Defense** — Air-gapped networks, ITAR

These environments prohibit:
- Sending prompts/data to external APIs
- Using cloud observability platforms
- Network access from build servers

---

## The Solution

Assay runs **100% locally**:

- ✅ No network calls during test execution
- ✅ No telemetry or data collection
- ✅ No external dependencies at runtime
- ✅ Single binary, no cloud account needed

---

## Architecture

### Cloud-Based Tools (Not Compliant)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Your Agent │ ──► │  Cloud API  │ ──► │  Dashboard  │
│   (Local)   │     │ (Internet)  │     │  (Internet) │
└─────────────┘     └─────────────┘     └─────────────┘
                           │
                    ❌ Data leaves
                       your network
```

### Assay (Compliant)

```
┌─────────────────────────────────────────────────────┐
│                  Your Network                        │
│                                                      │
│  ┌─────────────┐     ┌─────────────┐                │
│  │    Assay    │ ──► │   Reports   │                │
│  │   (Local)   │     │   (Local)   │                │
│  └─────────────┘     └─────────────┘                │
│         │                                            │
│         ▼                                            │
│  ┌─────────────┐                                    │
│  │   SQLite    │  ✅ Everything stays local         │
│  │   (Local)   │                                    │
│  └─────────────┘                                    │
└─────────────────────────────────────────────────────┘
```

---

## Setup

### 1. Install (Offline)

Download the binary on a connected machine:

```bash
# On connected machine
curl -L https://github.com/Rul1an/assay/releases/latest/download/assay-linux-x86_64.tar.gz -o assay.tar.gz
```

Transfer to air-gapped environment:

```bash
# On air-gapped machine
tar -xzf assay.tar.gz
sudo mv assay /usr/local/bin/
assay --version
```

### 2. Transfer Traces

Record sessions on a connected dev machine, then transfer:

```bash
# On dev machine
assay import --format mcp-inspector session.json

# Transfer
scp traces/session.jsonl air-gapped-server:/data/traces/
```

### 3. Run Tests (Offline)

```bash
# On air-gapped machine — no network needed
assay run \
  --config mcp-eval.yaml \
  --trace-file /data/traces/session.jsonl \
  --db :memory:
```

---

## CI/CD in Air-Gapped Environments

### Self-Hosted GitLab

```yaml
# .gitlab-ci.yml
agent-tests:
  stage: test
  image: internal-registry.corp/assay:v0.8.0
  script:
    - assay run --config mcp-eval.yaml --strict
  artifacts:
    reports:
      junit: .assay/reports/junit.xml
  tags:
    - air-gapped-runner
```

### Jenkins (On-Prem)

```groovy
pipeline {
    agent { label 'secure-zone' }
    stages {
        stage('Test') {
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

### Azure DevOps (Self-Hosted)

```yaml
pool:
  name: 'SecurePool'  # Self-hosted agent pool

steps:
  - script: assay run --config mcp-eval.yaml --strict
    displayName: 'Run Agent Tests'
```

---

## Compliance Mapping

### SOC 2

| Control | Assay Feature |
|---------|---------------|
| CC6.1 — Logical access | No external API access |
| CC7.2 — System monitoring | Local audit logs |
| CC8.1 — Change management | Policy-as-code, Git versioned |

### HIPAA

| Requirement | Assay Feature |
|-------------|---------------|
| §164.312(a) — Access control | Local execution only |
| §164.312(b) — Audit controls | Trace recording, local storage |
| §164.312(e) — Transmission security | No transmission |

### FedRAMP

| Control | Assay Feature |
|---------|---------------|
| AC-4 — Information flow | No outbound connections |
| AU-3 — Audit content | SARIF/JUnit reports |
| SC-7 — Boundary protection | Runs within boundary |

---

## Data Handling

### What Stays Local

| Data | Location |
|------|----------|
| Traces | `./traces/*.jsonl` |
| Policies | `./policies/*.yaml` |
| Config | `./mcp-eval.yaml` |
| Cache | `./.assay/store.db` |
| Reports | `./.assay/reports/` |

### No Telemetry

Assay collects **zero telemetry**:

- No usage analytics
- No crash reports
- No license phone-home
- No version checks

Verify with network monitoring:

```bash
# Run with network tracing
strace -e trace=network assay run --config mcp-eval.yaml 2>&1 | grep -E "connect|sendto"

# Output: (empty — no network calls)
```

---

## Offline Updates

### Check for Updates (Connected Machine)

```bash
curl -s https://api.github.com/repos/Rul1an/assay/releases/latest | jq -r '.tag_name'
```

### Download and Transfer

```bash
# Connected machine
curl -L https://github.com/Rul1an/assay/releases/download/v0.9.0/assay-linux-x86_64.tar.gz -o assay-0.9.0.tar.gz

# Transfer and install
scp assay-0.9.0.tar.gz air-gapped-server:/tmp/
ssh air-gapped-server 'tar -xzf /tmp/assay-0.9.0.tar.gz && sudo mv assay /usr/local/bin/'
```

---

## Docker (Air-Gapped Registry)

### Build and Push to Internal Registry

```bash
# On connected machine
docker pull ghcr.io/rul1an/assay:v0.8.0
docker tag ghcr.io/rul1an/assay:v0.8.0 internal-registry.corp/assay:v0.8.0
docker save internal-registry.corp/assay:v0.8.0 -o assay-image.tar

# Transfer and load
docker load -i assay-image.tar
docker push internal-registry.corp/assay:v0.8.0
```

### Use in CI

```yaml
image: internal-registry.corp/assay:v0.8.0
```

---

## Troubleshooting

### "Connection refused" Errors

If you see network errors, something is misconfigured. Assay should never make network calls:

```bash
# Verify no network in trace
assay run --config mcp-eval.yaml 2>&1 | grep -i network

# Should be empty
```

### Missing Dependencies

On minimal Linux installations:

```bash
# Install required libs (if not statically linked)
apt-get install -y libssl-dev ca-certificates
```

### Permission Issues

```bash
chmod +x /usr/local/bin/assay
```

---

## See Also

- [Installation](../getting-started/installation.md)
- [CI Integration](../getting-started/ci-integration.md)
- [Cache](../concepts/cache.md)
