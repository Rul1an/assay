# E2E Review Kit for assay-registry

This directory contains fixtures, mock stubs, and scripts for reviewing the pack registry implementation.

## Directory Structure

```
review-kit/
├── README.md                  # This file
├── fixtures/
│   ├── keys/
│   │   ├── test-signing-key.seed   # Deterministic Ed25519 seed (TEST ONLY)
│   │   └── test-key-id.txt         # Expected key ID
│   ├── packs/
│   │   ├── test-pack-1.0.0.yaml    # Sample pack content
│   │   └── test-pack-1.0.0.dsse    # Corresponding DSSE envelope
│   └── lockfile/
│       └── sample-lockfile.yaml    # Example lockfile v2
├── wiremock-stubs/
│   ├── get-pack-200.json           # Successful pack fetch
│   ├── get-pack-304.json           # Cache hit (Not Modified)
│   ├── get-pack-410.json           # Revoked pack
│   ├── get-pack-429.json           # Rate limited
│   └── keys-manifest.json          # Keys manifest response
└── scripts/
    ├── e2e.sh                      # Full E2E test script
    └── tamper.sh                   # Cache tampering test
```

## Quick Start

```bash
# Run E2E tests
./scripts/e2e.sh

# Test cache tampering detection
./scripts/tamper.sh
```

## Fixtures

### Keys

The test signing key uses a deterministic seed for reproducible testing:

```
Seed: 9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60
Key ID: sha256:<computed-from-spki>
```

**WARNING**: These keys are for testing only. Never use in production.

### Packs

Sample pack for testing:

```yaml
name: test-pack
version: "1.0.0"
kind: compliance
rules: []
```

### Lockfile

Example lockfile v2 format for testing lockfile verification.

## Wiremock Stubs

Use with [wiremock](https://wiremock.org/) for integration testing:

```bash
# Start wiremock with stubs
wiremock --port 8080 --root-dir ./wiremock-stubs
```

## Manual Review Checklist

1. **SPEC Compliance**
   - [ ] All endpoints return correct status codes
   - [ ] Headers match SPEC requirements
   - [ ] Error bodies follow JSON format

2. **Security**
   - [ ] Digest verification on every cache read
   - [ ] Signature verification for commercial packs
   - [ ] Unknown keys rejected

3. **Robustness**
   - [ ] 304 response uses cached content
   - [ ] 429 triggers exponential backoff
   - [ ] 410 provides safe version suggestion
