# Paused Approval Pattern

This directory holds the runtime-near Harness-side counterpart of the paused
HITL evidence pattern.

It is deliberately small:

- capture one paused approval result
- derive one `resume_state_ref` from serialized paused state
- emit one pause-only artifact
- validate that artifact against the bounded pattern

## Layout note

The original `P23A` plan sketch used `harness/patterns/paused_approval/`.

In this repository, `harness/` is the TypeScript package. The Python paused
approval pattern therefore lives under `patterns/paused_approval/` parallel to
the existing Python-facing repo-root surfaces instead of leaking Python modules
into the TypeScript package tree.

## Public API

The public imports are:

- `capture_paused_approval`
- `derive_resume_state_ref`
- `emit_pause_artifact`
- `validate_pause_artifact`

These are meant to work on plain Python dicts and serialized state values.

## Observed vs derived

This pattern keeps the same core distinction as the Assay-side reference docs:

- observed: `timestamp`, `interruptions`, naturally present reviewer aids
- derived: `resume_state_ref`

See [`FIELD_PRESENCE.md`](./FIELD_PRESENCE.md) for the explicit presence table.

## Canonical names

The canonical pause-only fields remain:

- `pause_reason`
- `interruptions`
- `call_id_ref`
- `resume_state_ref`

Known runtime aliases are accepted during capture, but the emitted artifact
always reduces back to those canonical names.

## Tolerated extensions

The pattern minimum is intentionally small.

When a caller needs reviewer aids without widening the artifact, these
extensions are tolerated:

- top-level: `policy_snapshot_hash`, `policy_decisions`
- per interruption item: `arguments_hash`

They are not part of the pattern minimum.

## Forbidden drift

The validator explicitly rejects pause-only drift such as:

- transcript history
- session identifiers
- `newItems`
- provider continuation hints
- raw serialized state inline
- resolved approval decision data

## Smoke path

The quickest end-to-end reviewer path is:

```bash
python3 patterns/paused_approval/smoke.py \
  --paused-result patterns/paused_approval/fixtures/raw/openai_agents_js.paused_result.json \
  --serialized-state patterns/paused_approval/fixtures/raw/openai_agents_js.serialized_state.txt
```

That path uses only the public imports and emits one clean pause-only artifact.

## Public import one-liner

```bash
python3 - <<'PY'
import json
from pathlib import Path

from patterns.paused_approval import (
    capture_paused_approval,
    derive_resume_state_ref,
    emit_pause_artifact,
)

raw = json.loads(Path("patterns/paused_approval/fixtures/raw/openai_agents_js.paused_result.json").read_text())
state = Path("patterns/paused_approval/fixtures/raw/openai_agents_js.serialized_state.txt").read_text()

captured = capture_paused_approval(raw)
artifact = emit_pause_artifact(
    captured,
    framework="openai_agents_js",
    schema="openai-agents-js.tool-approval-interruption.export.v1",
    surface="tool_approval_interruption_resumable_state",
    resume_state_ref=derive_resume_state_ref(state),
)
print(json.dumps(artifact, indent=2, sort_keys=True))
PY
```

That one-liner is intentionally close to how a second runtime would reuse the
same pattern later: observed capture in, derived state fingerprint in, bounded
artifact out.
