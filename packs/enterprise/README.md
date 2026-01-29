# Enterprise Packs

Enterprise compliance packs are available via [Assay Cloud](https://getassay.dev/enterprise) or direct licensing.

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

## Authentication

Enterprise packs require registry authentication:

```bash
# Via environment variable
export ASSAY_REGISTRY_TOKEN=your-token

# Or via config
assay config set registry.token your-token
```

## Pricing

See [getassay.dev/pricing](https://getassay.dev/pricing) for current plans.

## Contact

- **Sales:** enterprise@getassay.dev
- **Support:** support@getassay.dev

## Pack Development

Need a custom compliance pack for your industry or internal controls?

Contact us for pack development services (SOW-based).
