# SPLIT CHECKLIST - Wave 52 LiveKit Tool Action Step1

## Scope

Step1 freezes the LiveKit tool-action importer before any production split.

Allowed files:

- `docs/contributing/SPLIT-PLAN-wave52-livekit-tool-action.md`
- `docs/contributing/SPLIT-CHECKLIST-wave52-livekit-tool-action-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave52-livekit-tool-action-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave52-livekit-tool-action-step1.md`
- `scripts/ci/review-wave52-livekit-tool-action-step1.sh`
- `docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md`
- `docs/security/OWASP-MCP-TOP10-TEST-MAP.md`
- `scripts/ci/optional-public-api-drift.sh`
- `scripts/ci/mutation-smoke-pure-modules.sh`
- `scripts/ci/review-week8-sota-gates.sh`

Forbidden in Step1:

- edits under `crates/assay-cli/src/cli/commands/evidence/**`
- edits under `crates/assay-cli/tests/**`
- workflow edits
- generated file edits
- schema, hash, timestamp, pairing, or Trust Basis behavior changes

## Frozen Contracts

- CLI route and argument shape remain stable.
- `assay.receipt.livekit.tool_action.v1` event type remains stable.
- `assay.receipt.livekit.tool-action.v1` receipt schema remains stable.
- Reduced input schema remains `livekit.function-tools-executed.export.v1`.
- Raw transcript/audio/user/model/session/trace/telemetry imports remain rejected.
- Call/output pairing by `call_id` remains preferred over list order.
- JSONL multi-row import remains accepted.
- Raw argument/output values remain hashed or referenced, not embedded.
- LiveKit receipts remain importer-only and do not mutate Trust Basis external eval/decision/inventory claims.

## Reviewer Gate

Run:

```bash
bash scripts/ci/review-wave52-livekit-tool-action-step1.sh
```

The gate enforces docs+gate-only scope and runs the current LiveKit importer tests.
