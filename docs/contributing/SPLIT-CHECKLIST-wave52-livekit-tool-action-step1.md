# SPLIT CHECKLIST - Wave 52 LiveKit Tool Action Step1

## Scope

Step1 aligns and freezes the LiveKit tool-action importer before any production
split.

Allowed change areas:

- `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs`
- LiveKit input/receipt schemas under `crates/assay-cli/receipt-schemas/`
- mirrored LiveKit schema docs under `docs/reference/receipt-schemas/`
- LiveKit CLI/reference docs under `docs/reference/cli/evidence.md`
- P47 architecture docs
- `examples/livekit-tool-action-evidence/`
- Wave52 docs and reviewer gate
- Week 8 SOTA gate docs/scripts
- OWASP MCP Top 10 test coverage map

Forbidden in Step1:

- module extraction or file moves
- workflow edits
- generated file edits
- Trust Basis classifier changes
- public receipt-family matrix entries
- LiveKit endorsement or stable wire-contract claims

## Frozen Contracts

- CLI route and argument shape remain stable.
- `assay.receipt.livekit.tool_action.v1` event type remains stable.
- `assay.receipt.livekit.tool-action.v1` receipt schema remains importer-only.
- Reduced input schema remains `livekit.function-tools-executed.export.v1`.
- Raw transcript/audio/user/model/session/trace/telemetry imports remain rejected.
- Calls and outputs pair by LiveKit SDK list order.
- Complete per-index `call_id` pairs are checked for consistency.
- Partial `call_id` presence remains list-order paired.
- `null` outputs produce `completed=false` and no inferred `is_error`.
- JSONL multi-row import remains accepted.
- Raw argument/output values remain hashed or referenced, not embedded.
- LiveKit receipts remain importer-only and do not mutate Trust Basis external
  eval/decision/inventory claims.

## Reviewer Gate

Run:

```bash
bash scripts/ci/review-wave52-livekit-tool-action-step1.sh
```

The gate enforces Step1 scope and runs the current LiveKit importer contracts.
