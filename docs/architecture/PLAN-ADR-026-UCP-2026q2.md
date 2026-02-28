# PLAN — ADR-026 UCP Adapter Follow-up (2026q2)

## Intent
Follow ADR-026 with an open-core UCP adapter rollout using the same low-blast-radius discipline as ACP and A2A:
1. Step1 freeze the UCP contract and crate skeleton.
2. Step2 implement deterministic UCP translation with conformance fixtures.
3. Step3 close with checklist/review-pack and index sync.

This slice is Step1 only: contract + skeleton + reviewer gate. No runtime mapping.

## Scope (Step1 freeze)
In-scope:
- `assay-adapter-ucp` crate skeleton in the workspace
- UCP protocol metadata and capabilities contract
- Explicit non-goals for the first UCP MVP
- Reviewer gate for allowlist-only + workflow-ban

Out-of-scope (Step1):
- No UCP payload mapping logic
- No conformance fixtures yet
- No workflow changes
- No middleware, registry, or hosted control-plane features

## Target protocol surface (frozen)
Initial UCP MVP targets the governance-relevant subset only:
- discovery / catalog selection intent
- order and checkout lifecycle transitions
- post-purchase / fulfillment state references

This is intentionally narrower than the full UCP ecosystem surface.

## Upstream version anchor (frozen)
Step1 freezes against the current upstream release tag observed at implementation time:
- upstream project: `google-agentic-commerce/ucp`
- exact release tag: `v2026-01-23`

Because UCP uses date-tagged releases rather than classic semver, Step1 freezes the exact release tag instead of inventing a synthetic version range.

## Crate contract (Step1)
`assay-adapter-ucp` must:
- implement `ProtocolAdapter`
- expose protocol metadata for `ucp`
- declare support for the exact upstream release tag `v2026-01-23`
- keep raw payload preservation behind `AttachmentWriter`

Until Step2 mapping lands, `convert()` is allowed to return an explicit measurement/config error indicating that runtime translation is not yet implemented.

## Strictness expectations for Step2
When Step2 lands, UCP must follow the ADR-026 strictness contract:
- `strict`: malformed or unmappable critical data -> exit 2 in harnesses
- `lenient`: emit evidence plus lossiness + raw payload ref

## Initial event families (frozen for Step2)
Planned canonical event families:
- `assay.adapter.ucp.discovery.requested`
- `assay.adapter.ucp.order.requested`
- `assay.adapter.ucp.checkout.updated`
- `assay.adapter.ucp.fulfillment.updated`
- generic fallback: `assay.adapter.ucp.message`

## Non-goals
- Full UCP semantic coverage in Step1/Step2
- Live network ingestion
- Plugin/Wasm execution
- Enterprise middleware or approval workflows

## Acceptance criteria (Step1)
- `assay-adapter-ucp` exists as a compilable workspace crate
- crate exposes frozen protocol metadata and capabilities markers
- runtime conversion path is explicitly stubbed, not silently partial
- reviewer gate enforces allowlist-only and workflow-ban
