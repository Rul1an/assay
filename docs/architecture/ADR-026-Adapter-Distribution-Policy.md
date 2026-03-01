# ADR-026 Adapter Distribution Policy (v1)

## Intent
Freeze how the current ADR-026 adapter crates are distributed, with minimal blast radius:
- keep the new adapter crates in open core
- keep them available from source/workspace builds on `main`
- avoid taking on a crates.io support and semver contract before that surface is explicitly frozen

## Current State
The following crates now exist in the workspace on `main`:
- `assay-adapter-api`
- `assay-adapter-acp`
- `assay-adapter-a2a`
- `assay-adapter-ucp`

The release workflow can publish crates to crates.io through:
- `.github/workflows/release.yml`
- `scripts/ci/publish_idempotent.sh`

At the moment, that publish list does **not** include the adapter crates.

## Decision
For the current ADR-026 line, the adapter crates remain **workspace-internal open-core crates**.

This means:
- open-core availability is via the repository source tree and workspace builds
- no adapter crate is published to crates.io yet
- no external semver/support promise is made for the adapter crate API surface yet
- release tags must not silently start publishing adapter crates without a dedicated freeze/update slice

## In-Scope
- Freeze the distribution decision for the current line
- Clarify the distinction between open-core availability and crates.io publication
- Preserve the current release workflow behavior

## Out-of-Scope
- Any release workflow changes
- Adding adapter crates to `scripts/ci/publish_idempotent.sh`
- Defining a public crates.io support matrix
- docs.rs/readme polish for external crate consumers
- release-lane adapter integration work

## Distribution Contract (v1)
Until a new freeze slice says otherwise:
- `assay-adapter-api` is not published to crates.io
- `assay-adapter-acp` is not published to crates.io
- `assay-adapter-a2a` is not published to crates.io
- `assay-adapter-ucp` is not published to crates.io
- adapter crates may use workspace versioning without treating crates.io as the canonical distribution channel

## Rationale
Publishing now would create avoidable obligations before the surface is ready:
- `assay-adapter-api` would become a public semver contract
- protocol adapter crates would need an explicit external support/versioning story
- the release workflow would need a frozen publish order and migration rules

The current ADR-026 line froze:
- adapter API/workspace shape
- deterministic conversion behavior
- fixtures/tests/reviewer gates
- parser and host-boundary hardening

It did **not** freeze crates.io distribution as part of ADR-026.

## Criteria For A Future Publish Slice
A future distribution slice may publish adapter crates only after it freezes:
- public API stability for `assay-adapter-api`
- publish order and dependency policy in `scripts/ci/publish_idempotent.sh`
- external versioning/support expectations for ACP/A2A/UCP adapter crates
- release notes/docs for external crate consumers

## Acceptance Criteria
- The policy states that adapter crates stay workspace-internal for now
- The policy states that open-core availability does not imply crates.io publication
- A reviewer gate enforces allowlist-only scope and checks that the publish list still excludes the adapter crates
