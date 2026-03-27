# Wave50 Registry Auth Step3 Move Map

Step3 is a closure slice. No Rust bodies move in this step.

## Shipped Step2 shape being closed

- `crates/assay-registry/src/auth.rs`
  - stable facade for `TokenProvider` / `OidcProvider`
  - existing inline auth and OIDC tests remain in-place
- `crates/assay-registry/src/auth_next/providers.rs`
  - static/env precedence, auth-state helpers, provider constructors
- `crates/assay-registry/src/auth_next/oidc.rs`
  - GitHub Actions OIDC environment detection, request/exchange, retry/backoff
- `crates/assay-registry/src/auth_next/cache.rs`
  - cache hit, refresh, and clear behavior
- `crates/assay-registry/src/auth_next/headers.rs`
  - bearer/header and request-url helpers
- `crates/assay-registry/src/auth_next/diagnostics.rs`
  - error shaping for request/exchange failure paths

## Step3 closure assertion

Wave50 Step3 adds no new module cuts and performs no behavior cleanup. It only:
- records that Step2 shipped on `main`
- re-runs Step2 auth invariants
- re-runs downstream registry-client auth-header and unauthorized-response pins
- keeps the auth split audit trail complete
