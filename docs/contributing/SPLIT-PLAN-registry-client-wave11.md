# Wave 11 Plan - Registry Client Test Split

## Target

- hotspot: `crates/assay-registry/tests/registry_client.rs` (746 LOC, 26 async integration tests)
- objective: improve reviewability and failure localization via mechanical test decomposition
- constraint: zero behavior change

## Step sequence

1. Step1 freeze (this PR)
- docs + gates only
- no code moves
- no test semantics changes

2. Step2 mechanical split
- move tests into focused scenario modules
- keep a thin integration-test facade entrypoint
- preserve helper behavior and assertions 1:1

3. Step3 closure
- remove temporary duplication
- finalize docs + strict closure gates

## Test inventory (current `registry_client.rs`)

1. Pack fetch + status mapping
- success path, 304 not modified, 404 not found, 401 unauthorized
- revoked via header/body, rate-limited with `retry-after`

2. Registry metadata APIs
- `list_versions`, `get_pack_meta`, `fetch_keys`

3. Auth and request headers
- auth header present/absent behavior
- user-agent contract

4. Signature and sidecar behavior
- sidecar fetch success/404
- `fetch_pack_with_signature` success and error bubbling (500/invalid JSON)
- commercial sidecar-only signature requirement

5. Cache/digest/http contract behaviors
- 304 cache hit flows
- strong ETag format
- Vary/Cache-Control auth response behavior
- content digest vs canonical digest

6. Retry behavior
- retry on 429 with `retry-after`
- max retries exceeded

## Shared setup inventory

- helper: `create_test_client(mock_server)` builds authenticated client
- each test uses local `wiremock::MockServer::start()`
- common builders: `Mock::given(...).respond_with(ResponseTemplate::new(...))`

## External dependency inventory (flakiness surface)

- network: local in-process HTTP mock server only (`wiremock`), no external network calls
- filesystem: none
- env vars: none
- time: `Duration` and `Instant` used for retry timing assertions
- async runtime: Tokio integration tests

## Step2 target split map (mechanical)

- `crates/assay-registry/tests/registry_client.rs`
  - thin facade + module wiring only
- `crates/assay-registry/tests/registry_client/support/mod.rs`
  - shared client/mocks setup helpers
- `crates/assay-registry/tests/registry_client/scenarios/pack_fetch.rs`
- `crates/assay-registry/tests/registry_client/scenarios/meta_keys.rs`
- `crates/assay-registry/tests/registry_client/scenarios/auth_headers.rs`
- `crates/assay-registry/tests/registry_client/scenarios/signature.rs`
- `crates/assay-registry/tests/registry_client/scenarios/cache_digest.rs`
- `crates/assay-registry/tests/registry_client/scenarios/retry.rs`

## Contract invariants

- no behavior change
- no new network calls
- no retry policy changes
- no timeout/max_retries default changes
- all existing test names/assertions remain semantically equivalent

## Step1 allowlist

- `docs/contributing/SPLIT-PLAN-registry-client-wave11.md`
- `docs/contributing/SPLIT-CHECKLIST-registry-client-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-registry-client-step1.md`
- `scripts/ci/review-registry-client-step1.sh`

## Step2 allowlist preview

- `crates/assay-registry/tests/registry_client.rs`
- `crates/assay-registry/tests/registry_client/**`
- `docs/contributing/SPLIT-CHECKLIST-registry-client-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-registry-client-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-registry-client-step2.md`
- `scripts/ci/review-registry-client-step2.sh`
