# SPLIT PLAN - ADR-015 BYOS Phase 1 Closure

## Intent

Close the remaining ADR-015 Phase 1 delivery gap with a bounded wave.

ADR-015 Phase 1 is the largest open strategy-to-delivery gap in the repo.
Three of six Phase 1 items are shipped (`push`, `pull`, `list`). Three remain open:
`store-status`, structured config, and provider documentation.

This wave closes all three without touching the eval config pipeline or adding
new runtime capabilities outside the evidence store surface.

## Problem

ADR-015 was accepted January 2026 and scoped six Phase 1 deliverables.
As of 2026-03-15 on `main`:

| Item | Status |
|------|--------|
| `assay evidence push` | Shipped |
| `assay evidence pull` | Shipped |
| `assay evidence list` | Shipped |
| `assay evidence store-status` | Not shipped |
| Structured `assay.yaml` config | Not shipped (env vars + `--store` only) |
| Provider documentation | Not shipped (reference links in ADR-015 only) |

## Design decisions frozen in this plan

### 1. `store-status` command contract

```bash
assay evidence store-status --store s3://bucket/prefix
assay evidence store-status --store-config .assay/store.yaml
assay evidence store-status --format json
```

Args:
- `--store <url>` (env: `ASSAY_STORE_URL`) — same as push/pull/list
- `--store-config <path>` — path to store config YAML (default lookup below)
- `--format <json|table|plain>` — output format (default: `table`)

Precedence for store resolution:
1. `--store` CLI arg
2. `ASSAY_STORE_URL` env var
3. `--store-config` CLI arg (explicit path)
4. Default config lookup: `.assay/store.yaml`, then `assay-store.yaml`

Checks performed:
- Connectivity: can the bucket/container be reached?
- Credentials: do we have read access? write access?
- Bundle inventory: count and total size of stored bundles
- Object Lock: best-effort detection (default: `"unknown"`)

JSON output shape:

```json
{
  "reachable": true,
  "readable": true,
  "writable": true,
  "backend": "s3",
  "bucket": "my-evidence-bucket",
  "prefix": "assay/evidence",
  "bundle_count": 42,
  "total_size_bytes": 1048576,
  "object_lock": "unknown"
}
```

Exit codes:
- `0`: store reachable and usable
- `1`: connectivity or credential failure
- `2`: configuration error (no store URL, invalid spec)

### 2. Config design: option A (separate file)

`assay.yaml` remains the eval/test suite config (`EvalConfig`, `deny_unknown_fields`).
Evidence store config lives in a separate file.

Default lookup order:
1. `.assay/store.yaml`
2. `assay-store.yaml`

Config shape (frozen):

```yaml
# .assay/store.yaml
url: s3://my-bucket/assay/evidence

# Optional overrides (take precedence over env vars)
region: us-west-2
allow_http: false
path_style: false
```

Mapping to `StoreSpec`:
- `url` → `StoreSpec::parse(url)`
- `region` → overrides URL query param and `AWS_REGION` (same as `ASSAY_STORE_REGION`)
- `allow_http` → same as `ASSAY_STORE_ALLOW_HTTP`
- `path_style` → same as `ASSAY_STORE_PATH_STYLE`

The config file does not store credentials. Credentials remain in env vars
(`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, IAM roles, etc.).

Why option A:
- No change to `EvalConfig` or `assay-core` config loader
- No two-pass parsing complexity
- Clean separation: eval config vs infrastructure config
- Small bounded diff

### 3. Provider docs scope (Phase 1)

Three quickstart guides, one per supported backend class:

| Guide | Backend | Why |
|-------|---------|-----|
| AWS S3 | Canonical cloud | Most common production path |
| Backblaze B2 | S3-compatible BYOS | Explicit BYOS story with Object Lock |
| MinIO | Local/dev/test | Local development and CI testing |

Each guide: ~30-50 lines covering prerequisites, bucket setup, Assay config, and
verification with `store-status`.

Wasabi, Cloudflare R2, and other backends are explicitly deferred to a follow-up
docs pass. They are not blockers for Phase 1 closure.

## Wave structure

### Step 1 (this PR): Freeze

Deliverables:
- This plan document
- Step 1 checklist
- Step 1 review pack
- Review gate script

Allowed files:
- `docs/contributing/SPLIT-PLAN-adr015-byos-phase1-closure.md`
- `docs/contributing/SPLIT-CHECKLIST-adr015-byos-phase1-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-adr015-byos-phase1-step1.md`
- `scripts/ci/review-adr015-phase1-step1.sh`

No code changes. No workflow changes.

### Step 2: Implementation

Bounded code deliverables:
1. `store-status` command (CLI + store trait extension + implementation)
2. `StoreConfig` struct + YAML loader in `assay-evidence`
3. Config integration in push/pull/list/store-status commands
4. CLI integration tests (push/pull/list/store-status with `file://` backend)
5. Provider quickstart docs (S3, B2, MinIO)

Estimated diff: ~500-600 lines new, ~50 lines changed.

Frozen paths (Step 2 must not touch):
- `crates/assay-core/src/config.rs`
- `crates/assay-core/src/model/types.rs`
- `.github/workflows/*`

### Step 3: Closure

Docs + gate only:
1. ADR-015 Phase 1 checklist fully checked off
2. `ROADMAP.md`: BYOS from "Mostly complete" to "Complete"
3. `GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md`: update BYOS gap status
4. Review gate script for Step 3

No code changes.

## Explicitly out of scope

- `az://` and `gcs://` backend schemes
- `auto_push` and `verify_on_push` config flags
- GitHub Action store integration (ADR-015 Phase 2)
- Managed Evidence Store (ADR-015 Phase 3)
- `EvalConfig` or `assay-core` config loader changes
- Workflow file changes
- New CI required checks
