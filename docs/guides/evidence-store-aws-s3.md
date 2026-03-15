# Evidence Store: AWS S3

Quickstart for using AWS S3 as your BYOS evidence store.

## Prerequisites

- AWS account with S3 access
- AWS CLI configured (`aws configure`) or IAM role attached
- An S3 bucket (ideally with Object Lock enabled for WORM compliance)

## Bucket setup

```bash
aws s3 mb s3://my-assay-evidence --region us-east-1

# (Recommended) Enable Object Lock for WORM compliance
# Object Lock must be enabled at bucket creation time:
aws s3api create-bucket \
  --bucket my-assay-evidence \
  --region us-east-1 \
  --object-lock-enabled-for-object-lock-configuration
```

## Assay configuration

### Option A: Environment variables

```bash
export ASSAY_STORE_URL=s3://my-assay-evidence/assay/evidence
export AWS_ACCESS_KEY_ID=AKIA...
export AWS_SECRET_ACCESS_KEY=...
export AWS_REGION=us-east-1
```

### Option B: Config file

Create `.assay/store.yaml` (preferred) or `assay-store.yaml` in your project root:

```yaml
# .assay/store.yaml
url: s3://my-assay-evidence/assay/evidence
region: us-east-1
```

Credentials remain in environment variables or IAM role — never in the config file.

## Verify

```bash
assay evidence store-status
```

Expected output:

```
Evidence Store Status
====================

  Backend:      s3
  Bucket:       my-assay-evidence
  Prefix:       assay/evidence

  Reachable:    OK
  Readable:     OK
  Writable:     OK
  Object Lock:  unknown

  Bundles:      0
  Total size:   0 B
```

## Usage

```bash
assay evidence push bundle.tar.gz
assay evidence list
assay evidence pull --bundle-id sha256:...
```

## CI (GitHub Actions with OIDC)

For keyless CI authentication, see the [GitHub Action BYOS guide](github-action.md#byos-push).
