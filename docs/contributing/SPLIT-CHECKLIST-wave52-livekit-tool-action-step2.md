# SPLIT CHECKLIST - Wave 52 LiveKit Tool Action Step2

## Scope

Step2 mechanically splits the LiveKit tool-action importer behind the existing
CLI facade.

Allowed production files:

- `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs`
- `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/*.rs`

Allowed review files:

- `docs/contributing/SPLIT-CHECKLIST-wave52-livekit-tool-action-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave52-livekit-tool-action-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave52-livekit-tool-action-step2.md`
- `scripts/ci/review-wave52-livekit-tool-action-step2.sh`

Forbidden in Step2:

- workflow edits
- generated file edits
- schema file edits
- receipt payload shape changes
- Trust Basis classifier behavior changes
- CLI argument or command routing changes
- new LiveKit family-matrix or Trust Basis claims

## Frozen Contracts

- `cmd_livekit_tool_action(LiveKitToolActionArgs)` remains the public facade.
- `evidence import livekit-tool-action` CLI args remain unchanged.
- Event type remains `assay.receipt.livekit.tool_action.v1`.
- Receipt schema remains `assay.receipt.livekit.tool-action.v1`.
- Input schema remains `livekit.function-tools-executed.export.v1`.
- Pairing remains LiveKit SDK list order.
- `call_id` remains an optional per-index audit consistency check.
- `FunctionCallOutput | None` remains `completed=false` without inferred `is_error`.
- Raw arguments and output remain hashed or referenced, never embedded.
- Capture context/session identity remains rejected.
- LiveKit receipts remain importer-only and do not mutate Trust Basis claims.

## Reviewer Gate

Run:

```bash
bash scripts/ci/review-wave52-livekit-tool-action-step2.sh
```

The gate enforces scope containment, facade LOC, module boundaries, and the
current LiveKit importer contract tests.
