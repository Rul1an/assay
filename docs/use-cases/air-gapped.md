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

Assay can evaluate captured local evidence without a hosted Assay service. For an air-gapped
deployment, use local traces and policies, keep live replay and remote providers disabled, and
enforce the network boundary at the host or container layer. The release archive provides a CLI
binary and does not require an Assay cloud account.

---

## Architecture

### Hosted Data Path

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Your Agent │ ──► │  Cloud API  │ ──► │  Dashboard  │
│   (Local)   │     │ (Internet)  │     │  (Internet) │
└─────────────┘     └─────────────┘     └─────────────┘
                           │
                    ❌ Data leaves
                       your network
```

### Offline Assay Deployment

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
│  │   SQLite    │     Local evidence store            │
│  │   (Local)   │                                    │
│  └─────────────┘                                    │
└─────────────────────────────────────────────────────┘
```

---

## Setup

### 1. Install (Offline)

Download the binary on a connected machine:

```bash
# On a connected x86_64 Linux machine
curl -fLO https://github.com/Rul1an/assay/releases/download/v6.0.0/assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz
curl -fLO https://github.com/Rul1an/assay/releases/download/v6.0.0/assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

Transfer to air-gapped environment:

```bash
# On the air-gapped x86_64 Linux machine
tar -xzf assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz
sudo install -m 0755 assay-v6.0.0-x86_64-unknown-linux-gnu/assay /usr/local/bin/assay
assay --version
```

### 2. Transfer Traces

Record sessions on a connected dev machine, then transfer:

```bash
# On dev machine
assay import --format inspector session.json

# Transfer
scp traces/session.jsonl air-gapped-server:/data/traces/
```

### 3. Run Tests (Offline)

```bash
# On air-gapped machine — no network needed
assay run \
  --config eval.yaml \
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
  tags:
    - air-gapped-runner
  script:
    - assay run --config eval.yaml --strict
  artifacts:
    reports:
      junit: .assay/reports/junit.xml
```

### Jenkins (On-Prem)

```groovy
pipeline {
    agent { label 'secure-zone' }
    stages {
        stage('Test') {
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

### Azure DevOps (Self-Hosted)

```yaml
pool:
  name: 'SecurePool'  # Self-hosted agent pool

steps:
  - script: assay run --config eval.yaml --strict
    displayName: 'Run Agent Tests'
```

---

## Control Evidence Mapping

The following outputs can support an organization's control assessment. They do not establish
compliance, certification, or the behavior of systems outside the captured evidence.

### SOC 2

| Control | Assay Feature |
|---------|---------------|
| CC6.1 — Logical access | Locally enforced network boundary and policy configuration |
| CC7.2 — System monitoring | Local audit logs |
| CC8.1 — Change management | Policy-as-code, Git versioned |

### HIPAA

| Requirement | Assay Feature |
|-------------|---------------|
| §164.312(a) — Access control | Local policy and execution records |
| §164.312(b) — Audit controls | Trace recording, local storage |
| §164.312(e) — Transmission security | Host-level network controls and captured connection evidence |

### FedRAMP

| Control | Assay Feature |
|---------|---------------|
| AC-4 — Information flow | Host-level network controls and captured connection evidence |
| AU-3 — Audit content | SARIF/JUnit reports |
| SC-7 — Boundary protection | Deployment inside an operator-managed boundary |

---

## Data Handling

### What Stays Local

| Data | Location |
|------|----------|
| Traces | `./traces/*.jsonl` |
| Policies | `./policies/*.yaml` |
| Config | `./eval.yaml` |
| Cache | `./.assay/store.db` |
| Reports | `./.assay/reports/` |

### Network Boundary

Assay does not require an Assay-hosted telemetry or licensing service. Features configured with a
remote provider or exporter can make outbound calls, so an air-gapped deployment must leave those
features disabled and enforce its boundary independently. Verify the selected workflow under the
same host policy used in production:

```bash
# Example observation only; an empty trace is not proof that every relevant probe attached.
strace -f -e trace=network assay run --config eval.yaml --trace-file traces/session.jsonl
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
curl -fLO https://github.com/Rul1an/assay/releases/download/v6.0.0/assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz
curl -fLO https://github.com/Rul1an/assay/releases/download/v6.0.0/assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256
sha256sum -c assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz.sha256

# Transfer and install
scp assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz air-gapped-server:/tmp/
ssh air-gapped-server 'cd /tmp && tar -xzf assay-v6.0.0-x86_64-unknown-linux-gnu.tar.gz && sudo install -m 0755 assay-v6.0.0-x86_64-unknown-linux-gnu/assay /usr/local/bin/assay'
```

---

## Containers

Assay does not currently ship a runtime container image or a root `Dockerfile`. For an air-gapped
installation, mirror the verified release archive and checksum described above. The repository's
`docker/Dockerfile.ebpf-builder` builds the eBPF toolchain only; it is not an Assay runtime image.

Install the verified binary on the air-gapped CI runner host or bake that binary into an
organization-owned image using an internal build process. Assay does not provide or verify that
image recipe.

---

## Troubleshooting

### "Connection refused" Errors

If an offline workflow attempts a connection, inspect the selected provider, exporter, replay mode,
and host policy. Do not infer hermeticity from an empty application log; use host-level enforcement
and observation with an explicit positive control.

```bash
# Offline replay denies outbound access in the core policy layer by default.
assay replay --bundle run.assay-replay
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
