# G4-A — Phase 1 formal freeze (executable)

**Status:** Frozen for implementation **1b** (adapter-first).
**Parent:** [PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md](PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md).
**Repo snapshot:** Current [`assay-adapter-a2a`](../../crates/assay-adapter-a2a/) + [ADR026 fixtures](../../scripts/ci/fixtures/adr026/a2a/v0.2/) have **no** first-class upstream Agent Card / discovery **typed** columns; this freeze defines an **Assay-namespaced `attributes` contract** plus strict negatives so **1b** is not open-ended.

---

## 1. Definitive decision: `signature_material_visible` (v1)

**Decision:** **Deferred for semantic v1** — no upstream path maps to `true` in this freeze.

- **Emitted contract:** The key **`signature_material_visible`** is **always present** on `payload.discovery` and is **`false`** for all packets in v1.
- **Normative meaning:** “No bounded signature-related material signal in v1”; product and docs must **not** imply verification (see PLAN-G4 must-not table).
- **Revisit:** A later freeze (v1.1+) may add allowlisted paths **and** require positive/negative fixtures **before** `true` is allowed.

This matches **defer unless proven** and avoids weakening the seam with a half-defined path.

---

## 2. Payload placement and default shape (hard contract)

**Placement:** `discovery` is a **top-level sibling** on the **emitted canonical adapter payload** (same object as `agent`, `task`, `attributes`, `unmapped_fields_count`).

**Presence:** **`discovery` is always present** on every emitted event (diff-stable, no optional omission).

**Default values (when no rule sets a non-default):**

| Field | Default | JSON type |
|-------|---------|-----------|
| `agent_card_visible` | `false` | bool |
| `agent_card_source_kind` | `"unknown"` | string (enum) |
| `extended_card_access_visible` | `false` | bool |
| `signature_material_visible` | `false` | bool (fixed in v1; see §1) |

**Enum values for `agent_card_source_kind`:** `typed_payload` \| `attributes` \| `unmapped` \| `unknown`

---

## 3. Precedence rule (`agent_card_source_kind`)

**Order (highest wins first):**

1. `typed_payload`
2. `attributes`
3. `unmapped`
4. `unknown`

**Decision table (deterministic):**

| Situation | Result kind |
|-----------|-------------|
| A frozen **typed** upstream path matched for the same field’s visibility (see §4.1) **and** optional `attributes.assay_g4` also present | `typed_payload` |
| No typed match, but **allowlisted** `attributes.assay_g4` shape matched for that visibility | `attributes` |
| No typed and no attributes match, but a **frozen unmapped top-level key** rule matched (see §4.2) | `unmapped` |
| Otherwise | `unknown` |

**v1 note:** With this freeze, **`typed_payload` is unreachable for `agent_card_visible` / `extended_card_access_visible`** until a future freeze adds a typed upstream path (none in current ADR026 fixtures). Implementations must still implement precedence so adding typed paths does not change resolution order later.

---

## 4. Filled 1a freeze table (per field)

### 4.1 `agent_card_visible`

| | |
|--|--|
| **Exact source paths (v1)** | **Only** allowlisted Assay namespace under upstream `attributes`: key **`assay_g4`** (object). Inside it, key **`agent_card`** (object) with boolean **`visible`** required to consider promotion. **Path:** `attributes.assay_g4.agent_card.visible` → boolean `true` required for `agent_card_visible: true`. |
| **Minimal shape** | `attributes` is an object; `assay_g4` is an object; `agent_card` is an object; `visible` is JSON boolean. Anything else (string, missing `agent_card`, etc.) → **no promotion** (stay default `false`). |
| **May use `attributes`?** | **Yes**, only via the path above. No other attribute key may set this field. |
| **Multiple signals required?** | **No.** One matching allowlisted path with valid shape is **sufficient** for `true`. |
| **Explicit negatives (must not promote)** | (1) `agent.capabilities` present alone; (2) any non-allowlisted `attributes` key (e.g. `priority`, `session` per fixtures); (3) `attributes.assay_g4` present but wrong shape; (4) blob fragment without the nested boolean; (5) `unmapped_fields_count > 0` **alone**; (6) any heuristic on generic keys. |

**Hard rule — `agent.capabilities`:** Presence of **`agent.capabilities`** (or the capabilities event) **does not** set `agent_card_visible` to `true`. Capabilities remain **discovery-adjacent** only (PLAN Phase 0). Card visibility in v1 is **only** via the frozen **`attributes.assay_g4.agent_card.visible`** contract (or future typed path in a later freeze).

### 4.2 `extended_card_access_visible`

| | |
|--|--|
| **Exact source paths (v1)** | **Only** `attributes.assay_g4.extended_card_access.visible` → boolean `true` with same namespace rules: `assay_g4` and `extended_card_access` objects, `visible` boolean. |
| **Minimal shape** | Same discipline as §4.1 (nested objects + boolean). |
| **May use `attributes`?** | **Yes**, only this path. |
| **Multiple signals required?** | **No.** |
| **Explicit negatives** | Same style as §4.1; **no** inference from auth-like or generic keys. `agent_card_visible` and `extended_card_access_visible` are **independent** booleans (both may be true if both paths true). |

### 4.3 `signature_material_visible`

| | |
|--|--|
| **v1 decision** | **Deferred:** always **`false`**; **no** path maps to `true` (see §1). |
| **Future (non-normative)** | Any later freeze must list bounded paths + positive/negative fixtures before `true` is allowed. |

---

## 5. `unmapped_fields_count` and unmapped keys

- **`unmapped_fields_count`:** Never used **alone** to set any `discovery.*` field.
- **Unmapped top-level keys:** For this freeze, **no** `unmapped` branch sets visibility booleans; `agent_card_source_kind` may be `unmapped` only if a **future** freeze adds explicit rules. **v1:** treat as **never** matching unmapped for card signals — effectively **`unmapped` kind is unused for visibility in v1** unless only attributes/typed apply; if nothing matches → `unknown`.

Clarification: precedence row “unmapped” applies when a **frozen** rule names a specific unmapped key; **none** are frozen in this document → implement **no unmapped promotion** in v1.

---

## 6. Strict vs lenient

- **Same semantic defaults** in both modes: `discovery` always present with defaults in §2.
- **Difference:** strict may **reject** invalid packets per existing adapter rules; lenient may substitute unknown agent, etc. **Discovery** does not add hidden promotion in lenient mode: if `attributes.assay_g4` shape is invalid, visibility stays **false** and kind **`unknown`** (unless a higher-precedence source matched).

---

## 7. Two complete emitted JSON examples (full payload shape)

Field order below follows the **logical** sibling set: adapter metadata, protocol, agent, task, …, `attributes`, **`discovery`**, `unmapped_fields_count` (exact key order in JSON may vary; canonical ordering is defined in implementation/tests).

### 7.1 Weak positive (attributes-driven)

Upstream input must satisfy `attributes.assay_g4.agent_card.visible == true` (and shape). Emitted payload **illustrative**:

```json
{
  "adapter_id": "assay-adapter-a2a",
  "adapter_version": "3.3.0",
  "protocol": "a2a",
  "protocol_name": "a2a",
  "protocol_version": "0.2.0",
  "upstream_event_type": "agent.capabilities",
  "agent": {
    "id": "agent://planner",
    "name": "Planner",
    "role": "assistant",
    "capabilities": ["agent.describe", "artifacts.share", "tasks.update"]
  },
  "attributes": {
    "assay_g4": {
      "agent_card": { "visible": true }
    },
    "priority": "high",
    "session": "alpha"
  },
  "discovery": {
    "agent_card_visible": true,
    "agent_card_source_kind": "attributes",
    "extended_card_access_visible": false,
    "signature_material_visible": false
  },
  "unmapped_fields_count": 0
}
```

### 7.2 Fully default (no promotion)

Typical conversion from [a2a_happy_agent_capabilities.json](../../scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_agent_capabilities.json) **without** `assay_g4`:

```json
{
  "adapter_id": "assay-adapter-a2a",
  "adapter_version": "3.3.0",
  "protocol": "a2a",
  "protocol_name": "a2a",
  "protocol_version": "0.2.0",
  "upstream_event_type": "agent.capabilities",
  "agent": {
    "id": "agent://planner",
    "name": "Planner",
    "role": "assistant",
    "capabilities": ["agent.describe", "artifacts.share", "tasks.update"]
  },
  "attributes": {
    "priority": "high",
    "session": "alpha"
  },
  "discovery": {
    "agent_card_visible": false,
    "agent_card_source_kind": "unknown",
    "extended_card_access_visible": false,
    "signature_material_visible": false
  },
  "unmapped_fields_count": 0
}
```

---

## 8. Negative test matrix (minimum before merge of 1b)

| # | Case | Expected `discovery` |
|---|------|------------------------|
| N1 | Non-allowlisted `attributes` only (`priority`, `session`) | All bools false; `agent_card_source_kind` `unknown` |
| N2 | `attributes.assay_g4` present but wrong shape (e.g. string, or missing `agent_card`) | `agent_card_visible` false; kind `unknown` |
| N3 | `unmapped_fields_count > 0` only, no `assay_g4` | No discovery semantics from count; defaults unless §4 matches |
| N4 | Precedence: frozen typed path **not in v1** — use fixture with only `attributes.assay_g4` valid | `agent_card_source_kind` `attributes` |
| N5 | Strict vs lenient: same `attributes` without `assay_g4` | Same defaults as N1 |
| N6 | `signature_material_visible` | Always `false` in v1 |

---

## 9. assay-evidence scope

**No change to `assay-evidence` for G4-A v1** unless a **later** change is **objectively** required to represent a **new trust-basis or classification seam** that cannot be expressed in emitted adapter JSON and blocks **testability** of the adapter contract. **Not** for documentation parity, convenience, or pre-building P2c pack logic.

---

## 10. Link to implementation

- Implement **`attributes.assay_g4`** parsing only after `attributes` is read; validate shape; set `discovery` before `build_payload` inserts `unmapped_fields_count`.
- Add fixtures: one positive (§7.1), one default (§7.2), plus rows in §8.

---

## Changelog

| Date | Change |
|------|--------|
| 2026-03-25 | Initial executable freeze: Assay `attributes` contract, deferred `signature_material_visible`, defaults, precedence, JSON examples, negative matrix, assay-evidence line. |
