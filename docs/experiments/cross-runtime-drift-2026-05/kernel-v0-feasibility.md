# Kernel operation metadata note

> **Status:** follow-up diagnostic after the first live n=3 baseline,
> superseded by the kernel-event metadata implementation on the
> `codex/kernel-open-flags-return-values` branch. The first baseline
> showed why touched paths were not enough; the new Runner shape adds
> optional open flags, return values, access modes, and operation flags.
>
> **Conclusion:** old archives can only support event-kind counts and
> touched path / endpoint summaries. New archives can support honest
> read/write/create/truncate classification for successful `openat` /
> `openat2` events. The enriched line shape is now frozen in
> [`../../reference/runner/schema/kernel-event-v0.schema.json`](../../reference/runner/schema/kernel-event-v0.schema.json).

## Why this note exists

The first Slice 4 findings marked read/write/create/remove path drift
as a v2 follow-up. Before building a larger comparator, we checked
whether the committed kernel layer already carried enough evidence.

The first baseline did not. The relevant records looked like:

```json
{
  "schema": "assay.runner.kernel_event.v0",
  "run_id": "run_arm_a-openai_20260525T100626Z_1",
  "seq": 0,
  "pid": 146927,
  "event_type": 1,
  "kind": "openat",
  "value": "/path/to/package.json"
}
```

There was no open flag, syscall return value, access mode, file
descriptor correlation, or close/write/read pairing. A comparator that
labels a path as "read" or "write" from this older shape would be
guessing.

The Runner implementation now emits enriched open events shaped like:

```json
{
  "schema": "assay.runner.kernel_event.v0",
  "run_id": "run_arm_a-openai_...",
  "seq": 0,
  "pid": 146927,
  "event_type": 1,
  "kind": "openat",
  "value": "/path/to/fixture-output.txt",
  "flags": 577,
  "mode": 420,
  "return_value": 4,
  "access_mode": "write",
  "operation_flags": ["create", "truncate"],
  "status": "success"
}
```

## Live kernel-event counts

The committed live archives under [`runs/a0/`](runs/a0/) and
[`runs/b0/`](runs/b0/) are still useful. They show stable kernel-event
shape across all three pairs:

| Arm | Run | Total kernel events | `openat` events | `connect` events | Unique open paths | Unique connect endpoints |
|---|---|---:|---:|---:|---:|---:|
| OpenAI Agents | `run_arm_a-openai_20260525T100626Z_1` | 18 | 10 | 8 | 10 | 5 |
| OpenAI Agents | `run_arm_a-openai_20260525T100636Z_2` | 18 | 10 | 8 | 10 | 4 |
| OpenAI Agents | `run_arm_a-openai_20260525T100645Z_3` | 18 | 10 | 8 | 10 | 5 |
| Gemini GenAI | `run_arm_b-gemini_20260525T100327Z_1` | 27 | 10 | 17 | 10 | 17 |
| Gemini GenAI | `run_arm_b-gemini_20260525T100331Z_2` | 27 | 10 | 17 | 10 | 17 |
| Gemini GenAI | `run_arm_b-gemini_20260525T100334Z_3` | 27 | 10 | 17 | 10 | 17 |

The main findings doc already reflects the high-level result:

- filesystem drift is narrow and stable;
- SDK tool surface and invocation order are task-induced;
- live network rows are IP-based and therefore not provider-attributed
  under v0;
- Gemini has a larger observed connect surface in this particular run,
  but v0 cannot convert that into a provider-host claim.

## What old v0 can support

Old archives without open metadata can still support these
non-ambiguous rows:

| Candidate row | Evidence source | Safe interpretation |
|---|---|---|
| `kernel_event_kinds` | `layers/kernel.ndjson.kind` | Which kernel event types appeared in each archive. |
| `kernel_event_kind_counts` | `layers/kernel.ndjson.kind` counts | Event-count drift by kind, with no security verdict. |
| `kernel_open_paths_sequence` | ordered `openat.value` | Ordered touched-path shape, not read/write semantics. |
| `kernel_connect_sequence` | ordered `connect.value` | Ordered endpoint shape, still IP-based. |

Those rows make the old live drift report more transparent without
pretending old archives know more than they do.

## What the new metadata supports

The cross-runtime drift comparator now projects successful open events
into operation strings such as:

- `read:/path/to/fixture-input.txt`
- `write:/path/to/fixture-output.txt`
- `create:/path/to/fixture-output.txt`
- `truncate:/path/to/fixture-output.txt`

This is enough to distinguish the experiment workload's read tool from
its write tool. Full remove/unlink classification, fd-level read/write
byte counts, and close pairing remain out of scope until the monitor
captures those syscall families too.

## Reproduction

```bash
python3 - <<'PY'
import json
import tarfile
from pathlib import Path

root = Path("docs/experiments/cross-runtime-drift-2026-05/runs")
for arm in ["a0", "b0"]:
    print("ARM", arm)
    for archive in sorted((root / arm).glob("*/archive.tar.gz")):
        with tarfile.open(archive, "r:gz") as tf:
            member = tf.extractfile("layers/kernel.ndjson")
            assert member is not None
            events = [
                json.loads(line)
                for line in member.read().decode("utf-8").splitlines()
                if line.strip()
            ]
        kinds = {}
        for event in events:
            kinds[event["kind"]] = kinds.get(event["kind"], 0) + 1
        opens = sorted(
            {event["value"] for event in events if event.get("kind") == "openat"}
        )
        connects = sorted(
            {event["value"] for event in events if event.get("kind") == "connect"}
        )
        print(
            archive.parent.name,
            "events",
            len(events),
            "kinds",
            kinds,
            "open_paths",
            len(opens),
            "connects",
            len(connects),
        )
PY
```
