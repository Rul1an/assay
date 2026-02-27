# PLAN — ADR-026 A2A Adapter Follow-up (2026q2)

## Intent
Follow ADR-026 with an open-core A2A adapter rollout using the same low-blast-radius discipline as ACP:
1. Step1 freeze the A2A contract and crate skeleton.
2. Step2 implement deterministic A2A translation with conformance fixtures.
3. Step3 close with checklist/review-pack and index sync.

This slice is Step1 only: contract + skeleton + reviewer gate. No runtime mapping.

## Scope (Step1 freeze)
In-scope:
- `assay-adapter-a2a` crate skeleton in the workspace
- A2A protocol metadata and capabilities contract
- Explicit non-goals for the first A2A MVP
- Reviewer gate for allowlist-only + workflow-ban

Out-of-scope (Step1):
- No A2A payload mapping logic
- No conformance fixtures yet
- No workflow changes
- No middleware, registry, or hosted control-plane features

## Target protocol surface (frozen)
Initial A2A MVP targets the governance-relevant subset only:
- agent discovery / capability advertisement
- task delegation / task lifecycle
- artifact handoff references

This is intentionally narrower than the full A2A ecosystem surface.

## Crate contract (Step1)
`assay-adapter-a2a` must:
- implement `ProtocolAdapter`
- expose protocol metadata for `a2a`
- declare supported version range `>=0.2 <1.0`
- keep raw payload preservation behind `AttachmentWriter`

Until Step2 mapping lands, `convert()` is allowed to return an explicit measurement/config error indicating that runtime translation is not yet implemented.

## Strictness expectations for Step2
When Step2 lands, A2A must follow the ADR-026 strictness contract:
- `strict`: malformed or unmappable critical data -> exit 2 in harnesses
- `lenient`: emit evidence plus lossiness + raw payload ref

## Initial event families (frozen for Step2)
Planned canonical event families:
- `assay.adapter.a2a.agent.capabilities`
- `assay.adapter.a2a.task.requested`
- `assay.adapter.a2a.task.updated`
- `assay.adapter.a2a.artifact.shared`
- generic fallback: `assay.adapter.a2a.message`

## Non-goals
- Full A2A semantic coverage in Step1/Step2
- Live network ingestion
- Plugin/Wasm execution
- Enterprise middleware or approval workflows

## Acceptance criteria (Step1)
- `assay-adapter-a2a` exists as a compilable workspace crate
- crate exposes frozen protocol metadata and capabilities markers
- runtime conversion path is explicitly stubbed, not silently partial
- reviewer gate enforces allowlist-only and workflow-ban
