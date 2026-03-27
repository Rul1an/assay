# K1-A — Phase 1 formal freeze (executable)

**Status:** Frozen and now implemented on `main` for **K1-A Phase 1** (adapter-first).
**Parent:** [PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md](PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md).
**Repo snapshot:** Current [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) plus [ADR026 A2A fixtures](../../scripts/ci/fixtures/adr026/a2a/v0.2/) emit typed `task`, `message`, and canonical `assay.adapter.a2a.task.requested` events, and now also emit the first bounded top-level `handoff` object on `main`. `task.updated` remains mapped but is still **not** a positive source in v1, and the ADR026 fixture set still does **not** include a dedicated `task.updated` packet. This freeze defines **one bounded top-level `handoff` contract** from existing typed fields only, with explicit negatives and **no** reuse of `payload.discovery`.

### Contract honesty (product / review)

**K1-A v1 is typed, adapter-emitted visibility evidence only.** It is grounded in already-shipped A2A task/message fields, but it is **not** a full delegation graph, target-resolution surface, or handoff-correctness signal. Do **not** market this as “A2A delegation verified” or “route resolved” in v1. The only honest contract here is that a bounded **handoff / delegation-route surface is visible** in current typed A2A task-request packets, with limited task/message reference visibility on that same surface.

---

## 1. Definitive decision: typed-payload-only v1

**Decision:** `K1-A` v1 is **typed-payload only**.

Positive promotion for the new `handoff` seam is allowed **only** when all of the following hold:

1. the canonical emitted event type is **`assay.adapter.a2a.task.requested`**
2. the typed task payload contains **`task.kind == "delegation"`**

**Not allowed in v1:**

- `attributes`
- `payload.discovery`
- `unmapped_fields_count`
- unmapped top-level keys
- heuristics on `agent.role`, `artifact`, or generic message fallbacks

**Deferred in v1:**

- positive promotion from **`assay.adapter.a2a.task.updated`** until fixture-backed freeze evidence exists
- any target-agent identity, route-hop list, transfer success, or chain completeness

---

## 2. Payload placement and default shape (hard contract)

**Placement:** `handoff` is a **top-level sibling** on the emitted canonical adapter payload (same object as `agent`, `task`, `message`, `attributes`, `discovery`, and `unmapped_fields_count`).

**Presence:** **`handoff` is always present** on every emitted event.

**Default values (when no rule sets a non-default):**

| Field | Default | JSON type |
|-------|---------|-----------|
| `visible` | `false` | bool |
| `source_kind` | `"unknown"` | string (enum) |
| `task_ref_visible` | `false` | bool |
| `message_ref_visible` | `false` | bool |

**Enum values for `source_kind`:** `typed_payload` | `unknown`

**v1 reachability:** In this freeze, **`typed_payload`** is the **only** positive source kind. `unknown` is the default when the positive typed rule does not match. There is no `attributes`, `unmapped`, or fallback source kind in v1.

---

## 2b. Bounded meaning — may imply / must **not** imply

| Field | May imply (visibility / presence only) | Must **not** imply |
|-------|----------------------------------------|-------------------|
| `visible` | A bounded handoff / delegation-route surface was **observed** on a typed A2A task-request packet (`task.requested` + `task.kind == "delegation"`) | Delegation succeeded, was allowed, was correct, reached the right target, or completed a full chain |
| `source_kind` | Which **class** of source set the seam (`typed_payload` vs `unknown`) | Confidence, correctness, trust, or protocol-native completeness beyond the frozen rule |
| `task_ref_visible` | A typed `task.id` from the **upstream packet** was visible on a packet that already satisfied `handoff.visible` | Task identity verified, synthetic fallback accepted as real, or route completeness |
| `message_ref_visible` | A typed `message.id` from the **upstream packet** was visible on a packet that already satisfied `handoff.visible` | Message legitimacy, target identity, or delegation success |

**One-line semantics guardrail:** Every `handoff.*` boolean is a bounded **observed visibility** flag only. None of these fields mean the handoff was valid, authorized, complete, trusted, or successful.

---

## 3. Source rule and decision table

**Positive v1 source rule (hard requirement):**

| Situation | Result |
|-----------|--------|
| Canonical event type is `assay.adapter.a2a.task.requested` **and** typed `task.kind == "delegation"` | `handoff.visible = true`, `handoff.source_kind = "typed_payload"` |
| Same as above **plus** upstream typed `task.id` present | `handoff.task_ref_visible = true` |
| Same as above **plus** upstream typed `message.id` present | `handoff.message_ref_visible = true` |
| Lenient mode substituted synthetic `task.id = "unknown-task"` | `handoff.task_ref_visible = false` |
| Any other packet shape or event class | all defaults; `handoff.source_kind = "unknown"` |

**Explicit v1 non-promotion rule:** `assay.adapter.a2a.task.updated` is **not** a positive source in this freeze, even though the adapter already maps it. A later freeze may widen the source rule only after a fixture-backed update.

---

## 4. Filled freeze table (per field)

### 4.1 `handoff.visible`

| | |
|--|--|
| **Exact source paths (v1)** | Canonical event type **`assay.adapter.a2a.task.requested`** plus typed payload path **`task.kind`** with string value **`"delegation"`**. |
| **Minimal shape** | The packet maps to the canonical task-request event, `task` is an object, and `task.kind` is a JSON string equal to `delegation`. |
| **May use `attributes`?** | **No.** |
| **May use `payload.discovery`?** | **No.** |
| **Multiple signals required?** | **Yes.** Event class **and** `task.kind == "delegation"` are both required. |
| **Explicit negatives (must not promote)** | (1) `artifact.shared` even if `task.id` exists; (2) generic `assay.adapter.a2a.message` fallback even if `message.id` exists; (3) `agent.capabilities`; (4) missing `task.kind`; (5) non-`delegation` `task.kind`; (6) `agent.role` alone; (7) `attributes` blobs; (8) `payload.discovery`; (9) `unmapped_fields_count` alone; (10) `task.updated` in v1. |

### 4.2 `handoff.source_kind`

| | |
|--|--|
| **Exact source rule (v1)** | `typed_payload` when §4.1 matches; otherwise `unknown`. |
| **Enum surface** | `typed_payload` | `unknown` |
| **Explicit negatives** | No other source kind may appear in v1. In particular, `attributes`, `unmapped`, and fallback generic-message paths are **not** valid `handoff` source kinds. |

### 4.3 `handoff.task_ref_visible`

| | |
|--|--|
| **Exact source paths (v1)** | Upstream typed `task.id` mapped into canonical `payload.task.id`, **and** the packet already satisfies §4.1 for `handoff.visible`. |
| **Minimal shape** | Upstream `task.id` is a JSON string and was present before any lenient substitution. |
| **Synthetic fallback rule** | The current lenient adapter may substitute `task.id = "unknown-task"` for task events. That synthetic value **must not** set `handoff.task_ref_visible = true`. |
| **Explicit negatives** | (1) synthetic `unknown-task`; (2) `artifact.shared` task IDs; (3) any packet where `handoff.visible = false`. |

### 4.4 `handoff.message_ref_visible`

| | |
|--|--|
| **Exact source paths (v1)** | Upstream typed `message.id` mapped into canonical `payload.message.id`, **and** the packet already satisfies §4.1 for `handoff.visible`. |
| **Minimal shape** | Upstream `message.id` is a JSON string. |
| **Explicit negatives** | (1) generic `assay.adapter.a2a.message` fallback; (2) packets where `handoff.visible = false`; (3) synthetic `unknown-message` on generic fallback packets. |

---

## 5. Strict vs lenient

- **Strict mode:** current adapter behavior already rejects task packets without `task.id`; that remains true. No emitted `handoff` object exists because the whole packet is rejected.
- **Lenient mode:** `handoff.visible` may still be `true` when the typed route surface exists (`task.requested` + `task.kind == "delegation"`), even if `task.id` is missing. In that case:
  - `handoff.source_kind = "typed_payload"`
  - `handoff.task_ref_visible = false`
  - `handoff.message_ref_visible` follows upstream typed `message.id` only
- **Invalid event-type fallback:** packets that fall back to `assay.adapter.a2a.message` keep the full `handoff` default object, even if `message.id` or synthetic `unknown-message` is present.

---

## 6. Complete emitted JSON examples (illustrative)

**Build note:** `adapter_version` uses `3.3.0` as an illustration; real emission uses `CARGO_PKG_VERSION`. Reviewers should treat **presence and shape** of top-level keys as normative here, not the patch digit.

### 6.1 Typed positive (`task.requested` + `delegation`)

Fixture source: [a2a_happy_task_requested.json](../../scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_task_requested.json)

```json
{
  "adapter_id": "assay-adapter-a2a",
  "adapter_version": "3.3.0",
  "protocol": "a2a",
  "protocol_name": "a2a",
  "protocol_version": "0.2",
  "upstream_event_type": "task.requested",
  "agent": {
    "id": "agent://coordinator",
    "role": "orchestrator"
  },
  "task": {
    "id": "task-123",
    "status": "requested",
    "kind": "delegation"
  },
  "message": {
    "id": "msg-1",
    "role": "assistant"
  },
  "attributes": {
    "channel": "web",
    "priority": "urgent"
  },
  "discovery": {
    "agent_card_visible": false,
    "agent_card_source_kind": "unknown",
    "extended_card_access_visible": false,
    "signature_material_visible": false
  },
  "handoff": {
    "visible": true,
    "source_kind": "typed_payload",
    "task_ref_visible": true,
    "message_ref_visible": true
  },
  "unmapped_fields_count": 0
}
```

### 6.2 Lenient partial visibility (`task.id` missing)

Fixture source: [a2a_negative_missing_task_id.json](../../scripts/ci/fixtures/adr026/a2a/v0.2/a2a_negative_missing_task_id.json) in **lenient** mode

```json
{
  "adapter_id": "assay-adapter-a2a",
  "adapter_version": "3.3.0",
  "protocol": "a2a",
  "protocol_name": "a2a",
  "protocol_version": "0.2",
  "upstream_event_type": "task.requested",
  "agent": {
    "id": "agent://coordinator"
  },
  "task": {
    "id": "unknown-task",
    "status": "requested",
    "kind": "delegation"
  },
  "message": {
    "id": "msg-2"
  },
  "discovery": {
    "agent_card_visible": false,
    "agent_card_source_kind": "unknown",
    "extended_card_access_visible": false,
    "signature_material_visible": false
  },
  "handoff": {
    "visible": true,
    "source_kind": "typed_payload",
    "task_ref_visible": false,
    "message_ref_visible": true
  },
  "unmapped_fields_count": 1
}
```

### 6.3 Fully default (`artifact.shared`)

Fixture source: [a2a_happy_artifact_shared.json](../../scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_artifact_shared.json)

```json
{
  "adapter_id": "assay-adapter-a2a",
  "adapter_version": "3.3.0",
  "protocol": "a2a",
  "protocol_name": "a2a",
  "protocol_version": "0.3.1",
  "upstream_event_type": "artifact.shared",
  "agent": {
    "id": "agent://worker"
  },
  "task": {
    "id": "task-123"
  },
  "artifact": {
    "id": "artifact-7",
    "name": "plan.md",
    "media_type": "text/markdown"
  },
  "discovery": {
    "agent_card_visible": false,
    "agent_card_source_kind": "unknown",
    "extended_card_access_visible": false,
    "signature_material_visible": false
  },
  "handoff": {
    "visible": false,
    "source_kind": "unknown",
    "task_ref_visible": false,
    "message_ref_visible": false
  },
  "unmapped_fields_count": 0
}
```

---

## 7. Negative test matrix (implemented minimum on `main`)

| # | Case | Expected `handoff` |
|---|------|--------------------|
| N1 | `artifact.shared` with `task.id` present | all defaults |
| N2 | lenient invalid `event_type` falling back to `assay.adapter.a2a.message` | all defaults, even if `message.id` or `unknown-message` exists |
| N3 | strict `task.requested` packet missing `task.id` | measurement error; no emitted packet |
| N4 | lenient `task.requested` packet with `task.kind = delegation` but missing `task.id` | `visible = true`, `source_kind = "typed_payload"`, `task_ref_visible = false` |
| N5 | `task.requested` packet with missing or non-`delegation` `task.kind` | all defaults |
| N6 | `agent.role`, `attributes`, `payload.discovery`, or `unmapped_fields_count` alone | all defaults |
| N7 | `task.updated` packet in v1 | all defaults until a later freeze widens the positive rule |

**Current coverage note:** the first K1-A adapter slice on `main` now includes explicit adapter-level
tests for N1, N2, N4, N5, and N7, plus helper-level direct coverage for the source-rule default
paths. The ADR026 fixture set itself still does not include a dedicated `task.updated` packet, so
future fixture-parity work remains optional rather than a blocker for the shipped v1 seam.

---

## 8. `assay-evidence` scope

**No change to `assay-evidence` in K1-A v1.** This freeze is about canonical adapter-emitted evidence shape only. Any later Trust Basis, Trust Card, or pack follow-up must be justified separately after the `handoff` seam is real.

---

## 9. Link to implementation

- Implement `handoff` in the A2A adapter as a **separate top-level sibling**, not as an extension of `payload.discovery`.
- Source logic must be grounded in current typed fields from:
  - [`mapping.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/mapping.rs)
  - [`convert.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/convert.rs)
  - [`payload.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/payload.rs)
- Tests must cover:
  - typed positive from `a2a_happy_task_requested.json`
  - strict failure on missing `task.id`
  - lenient partial visibility on missing `task.id`
  - no promotion from `artifact.shared`
  - no promotion from generic message fallback
  - explicit non-promotion from `task.updated` in v1
- Canonical stability should be asserted with repeated conversion plus golden digests over the emitted `handoff` sub-object.

---

## 10. Reviewer checklist (contract closure)

| Question | Expected answer |
|----------|-----------------|
| Is `handoff` a single bounded top-level surface? | **Yes** |
| Does v1 promote only from typed `task.requested` + `task.kind == "delegation"`? | **Yes** |
| Does v1 avoid `attributes`, `payload.discovery`, unmapped keys, and heuristics? | **Yes** |
| Is synthetic `unknown-task` blocked from counting as `task_ref_visible`? | **Yes** |
| Is `task.updated` explicitly non-promoting in v1? | **Yes** |
| Does the doc stay at visibility language, not correctness or trust language? | **Yes** |
