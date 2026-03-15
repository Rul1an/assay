# Evidence Store: Backblaze B2

Quickstart for using Backblaze B2 as your BYOS evidence store.

B2 is S3-compatible with native Object Lock support and free egress via Cloudflare.

## Prerequisites

- Backblaze account
- B2 bucket with **Object Lock enabled** (must be set at creation time)
- Application key with read/write access to the bucket

## Bucket setup

1. Create a bucket in the [B2 Console](https://secure.backblaze.com/b2_buckets.htm)
2. Enable **Object Lock** during creation (cannot be enabled later)
3. Create an [Application Key](https://secure.backblaze.com/app_keys.htm) scoped to the bucket

Note your endpoint URL — it follows the pattern `s3.<region>.backblazeb2.com`.

## Assay configuration

### Option A: Environment variables

```bash
export ASSAY_STORE_URL=s3://my-assay-evidence/assay/evidence
export AWS_ACCESS_KEY_ID=<your-b2-key-id>
export AWS_SECRET_ACCESS_KEY=<your-b2-application-key>
export AWS_ENDPOINT=https://s3.us-west-002.backblazeb2.com
export ASSAY_STORE_REGION=us-west-002
```

### Option B: Config file

```yaml
# .assay/store.yaml
url: s3://my-assay-evidence/assay/evidence
region: us-west-002
```

Set credentials and endpoint via environment:

```bash
export AWS_ACCESS_KEY_ID=<your-b2-key-id>
export AWS_SECRET_ACCESS_KEY=<your-b2-application-key>
export AWS_ENDPOINT=https://s3.us-west-002.backblazeb2.com
```

## Verify

```bash
assay evidence store-status
```

## Usage

```bash
assay evidence push bundle.tar.gz
assay evidence list --format table
assay evidence pull --bundle-id sha256:...
```

## Cost

B2 pricing (as of 2026):
- Storage: $0.006/GB/month
- Egress: $0.01/GB (free via Cloudflare CDN)
- Object Lock: included, no extra cost
