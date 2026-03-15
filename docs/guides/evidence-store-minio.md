# Evidence Store: MinIO (Local / Dev / Test)

Quickstart for using MinIO as a local evidence store for development and testing.

MinIO is an S3-compatible object store that runs locally.

## Prerequisites

- Docker (recommended) or MinIO binary

## Start MinIO

```bash
docker run -d --name minio \
  -p 9000:9000 -p 9001:9001 \
  -e MINIO_ROOT_USER=minioadmin \
  -e MINIO_ROOT_PASSWORD=minioadmin \
  quay.io/minio/minio server /data --console-address ":9001"
```

Create a bucket:

```bash
# Using the MinIO client (mc)
mc alias set local http://localhost:9000 minioadmin minioadmin
mc mb local/assay-evidence
```

Or create the bucket via the MinIO Console at `http://localhost:9001`.

## Assay configuration

### Option A: Environment variables

```bash
export ASSAY_STORE_URL=s3://assay-evidence/evidence
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_ENDPOINT=http://localhost:9000
export ASSAY_STORE_REGION=us-east-1
export ASSAY_STORE_ALLOW_HTTP=1
export ASSAY_STORE_PATH_STYLE=1
```

### Option B: Config file

```yaml
# .assay/store.yaml
url: s3://assay-evidence/evidence
region: us-east-1
allow_http: true
path_style: true
```

Set credentials via environment:

```bash
export AWS_ACCESS_KEY_ID=minioadmin
export AWS_SECRET_ACCESS_KEY=minioadmin
export AWS_ENDPOINT=http://localhost:9000
```

## Verify

```bash
assay evidence store-status
```

## Usage

```bash
assay evidence push bundle.tar.gz
assay evidence list
assay evidence pull --bundle-id sha256:...
```

## File backend alternative

For the simplest local testing (no Docker), use the `file://` backend:

```bash
assay evidence push bundle.tar.gz --store file:///tmp/assay-store
assay evidence list --store file:///tmp/assay-store
assay evidence store-status --store file:///tmp/assay-store
```
