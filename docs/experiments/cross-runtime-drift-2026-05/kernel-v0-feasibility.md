# Kernel v0 feasibility note

> **Status:** follow-up diagnostic after the live n=3 baseline. This
> note inspects the committed `layers/kernel.ndjson` files and decides
> what the current Runner kernel-event v0 shape can and cannot support
> for the cross-runtime drift experiment.
>
> **Conclusion:** kernel v0 can support event-kind counts and touched
> path / endpoint summaries. It cannot support honest read/write/create
> / remove classification yet, because each event carries only `kind`,
> `value`, `seq`, `pid`, `run_id`, `schema`, and numeric `event_type`.

## Why this note exists

The Slice 4 findings mark read/write/create/remove path drift as a v2
follow-up. Before building a larger comparator, we need to know whether
the committed kernel layer already carries enough evidence.

It does not. The relevant records currently look like:

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

There is no open flag, syscall return value, access mode, file
descriptor correlation, or close/write/read pairing. A v2 comparator
that labels a path as "read" or "write" from this shape would be
guessing.

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

## What v0 can support next

A small comparator extension could add these non-ambiguous rows:

| Candidate row | Evidence source | Safe interpretation |
|---|---|---|
| `kernel_event_kinds` | `layers/kernel.ndjson.kind` | Which kernel event types appeared in each archive. |
| `kernel_event_kind_counts` | `layers/kernel.ndjson.kind` counts | Event-count drift by kind, with no security verdict. |
| `kernel_open_paths_sequence` | ordered `openat.value` | Ordered touched-path shape, not read/write semantics. |
| `kernel_connect_sequence` | ordered `connect.value` | Ordered endpoint shape, still IP-based. |

Those rows would make the live drift report more transparent without
pretending v0 knows more than it does.

## What requires a Runner schema change

Read/write/create/remove path classification needs more evidence in the
kernel layer, for example:

- syscall-specific open flags for `openat` / `openat2`;
- return-value success/failure;
- file descriptor correlation if later `read`, `write`, or `close`
  events are used;
- normalized operation category emitted by Runner after parsing the raw
  syscall fields.

Until then, the correct publication language is "filesystem paths
touched", not "files read" or "files written".

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
