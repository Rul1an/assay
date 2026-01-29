# Enterprise Packs

Enterprise compliance packs are available via [Assay Registry](https://getassay.dev/enterprise) or direct licensing.

## Available Packs

| Pack | Description | Coverage |
|------|-------------|----------|
| `eu-ai-act-pro` | Extended EU AI Act compliance | Articles 12(3), 19, biometric rules |
| `soc2-pro` | SOC 2 Type II control mapping | Trust Service Criteria |
| `hipaa-pro` | HIPAA compliance for healthcare AI | PHI handling, audit controls |

## Usage

Enterprise packs use the same interface as open packs:

```bash
assay evidence lint --pack eu-ai-act-pro@1.2.0 bundle.tar.gz
```

**Note:** Version is required (`@1.2.0`). `@latest` is not supported for reproducibility.

## Authentication

Enterprise packs require registry authentication.

### Token Authentication

```bash
# Via environment variable
export ASSAY_REGISTRY_TOKEN=ast_...

# Or via config
assay config set registry.token ast_...
```

### OIDC Authentication (GitHub Actions)

```yaml
permissions:
  id-token: write

steps:
  - name: Authenticate to Assay Registry
    run: |
      TOKEN=$(curl -s -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
        "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=https://registry.getassay.dev" | jq -r '.value')
      echo "ASSAY_REGISTRY_TOKEN=$TOKEN" >> $GITHUB_ENV

  - name: Lint with enterprise pack
    run: assay evidence lint --pack eu-ai-act-pro@1.2.0 bundle.tar.gz
```

## Resolution Order

When you specify `--pack <ref>`, the CLI resolves in this order:

1. **Local path** (`./custom.yaml`) — file on disk
2. **Bundled pack** (`eu-ai-act-baseline`) — in `packs/open/`
3. **Registry** (`eu-ai-act-pro@1.2.0`) — fetch from Assay Registry
4. **BYOS** (`s3://bucket/packs/...`) — fetch from your storage

## Integrity

All packs are verified via SHA256 digest (JCS canonical). The registry returns
`X-Pack-Digest` header; CLI verifies before use.

## Pricing

See [getassay.dev/pricing](https://getassay.dev/pricing) for current plans.

## Contact

- **Sales:** enterprise@getassay.dev
- **Support:** support@getassay.dev

## Custom Packs

Need a custom compliance pack for your industry or internal controls?
Contact us for pack development services (SOW-based).

## Technical Specification

See [SPEC-Pack-Registry-v1](../../docs/architecture/SPEC-Pack-Registry-v1.md) for the full protocol specification.
