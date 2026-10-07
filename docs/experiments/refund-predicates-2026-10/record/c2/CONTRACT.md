# C2 synthetic wire contract v0

Synthetic conventions; not a provider protocol.

## Input packet (schema `refund.synthetic-c2.v0`)

**Root.** The root object has exactly these keys:

| Key | Value |
|---|---|
| `schema` | `"refund.synthetic-c2.v0"` |
| `case_id` | identifier |
| `source_class` | `"synthetic_fixture"` |
| `custody` | identifier: who controls the effect store; fixture-declared, not authenticated |
| `effect_coverage` | `"complete"` or `"incomplete"` |
| `admissions` | array, 1..100 entries |
| `events` | array, 1..1000 entries |

**Identifiers** are ASCII matching `[A-Za-z0-9_.:-]{1,128}`. **Digests** are 64 lowercase hex characters.
**Integers** are integer number tokens (see "Numbers and depth"), never booleans. `amount` is in 1..2^63-1 minor units; `currency` is
three uppercase ASCII letters. Every object is closed: a missing key or an unknown key refuses. `null` is allowed
only where stated below.

**Admission record:** exactly `admission_id` (identifier), `approval_digest` (digest), `operation_id` (identifier)
and `reissue_of` (an earlier `admission_id`, or `null`).

**Events.** Every event has `id` (identifier), `seq` (integer) and `kind`, plus the fields for its kind:

| `kind` | Additional fields |
|---|---|
| `dispatch` | `approval_digest` (digest or `null`), `workflow_key` (identifier or `null`) |
| `observation` | `effect_id` (identifier), `dispatch_id` (identifier), `amount`, `currency`, `status` (`pending`, `committed`, `failed` or `reversed`) |
| `epoch_close` | `closer` (identifier) |

## Binding rules (packet-level validity)

- `seq` equals the array index + 1. Event `id`s are unique among events. `admission_id`s are unique among
  admissions. `approval_digest`s are unique among admissions. The namespaces of event ids, admission ids,
  operation ids and effect ids are separate.
- **Admissions.** The first admission of an `operation_id` (in array order) has `reissue_of: null`. Every later
  admission of the same `operation_id` has `reissue_of` naming an earlier admission **of the same operation**. A
  reissue adds an approval binding; it never replaces the original admission.
- **Observations.** `dispatch_id` must name a `dispatch` event earlier in `events`. A missing dispatch is dangling; a
  later one is a forward reference.
- At most one `epoch_close`.
- All observations of one `effect_id` agree on `dispatch_id`, `amount` and `currency`.
- **Lifecycle.** Collapse consecutive equal `status` values of one `effect_id` (in `seq` order). The collapsed path
  must be one of:
  - `[pending]`
  - `[pending, committed]`
  - `[pending, failed]`
  - `[pending, committed, reversed]`
  - `[pending, reversed]`
  - `[committed]`
  - `[committed, reversed]`
  - `[failed]`
  - `[reversed]`

A dispatch whose `approval_digest` is `null`, or absent from `admissions`, is accepted as **unadmitted**. That is
not a refusal.

## Numbers and depth (representation-independent)

These rules are defined on the JSON text, not on any decoder's numeric type.

- **Number tokens.** A number token is classified by its characters:
  - an **integer token** matches `-?(0|[1-9][0-9]*)`;
  - a **non-integer token** is any other valid JSON number token, that is, one with a fraction part and/or an
    exponent (`1000.0`, `1e2`, `1e9999`).
- **Parse stage, numbers.** The parse stage refuses only:
  - an integer token with more than 4300 digits (the sign excluded);
  - text that is not valid JSON, including the barewords `NaN`, `Infinity` and `-Infinity`.

  A non-integer token is **never** a parse refusal, whatever its magnitude. A decoder must not convert it to
  binary64 and refuse on overflow. `1e9999` is a valid non-integer token.
- **Schema stage, numbers.** The C2 schema has no non-integer number field. Every non-integer token, wherever it
  appears, is therefore a schema refusal:
  - as a value where an integer is required, a type refusal;
  - anywhere else, a type or closed-shape refusal.

  An integer token of at most 4300 digits outside its field's range (for example `amount` 0 or above 2^63-1) is a
  schema refusal.
- **Structural depth.** The root value is at depth 1 if it is an object or array. Each object or array nested inside
  a container adds 1. Scalars (strings, numbers, booleans, null) add nothing, and brackets inside strings are
  characters, not structure. A document whose maximum depth exceeds 32 is a parse refusal. A depth of exactly 32 is
  accepted by the parse stage. A valid C2 packet has depth 3 (root object, `events` array, event object).

## Refusal stages (exit 2, empty stdout, first stderr line `refused: CATEGORY: ...`)

The stages run in this order, and the first failing stage names the category. Free diagnostic text is not stable.

| Category | Covers |
|---|---|
| `io` | CLI arity, file read |
| `parse` | over 1048576 bytes, invalid UTF-8, BOM, invalid JSON (including `NaN`/`Infinity` barewords), trailing data, duplicate keys, integer tokens over 4300 digits, structural depth over 32 |
| `schema` | closed shapes, types, enums, identifier/digest/currency syntax, null placement, `amount` range, array counts, bool-as-integer, every non-integer number token |
| `binding` | everything in "Binding rules" above |

## Semantics (DESIGN §2.6)

**Attribution.**
- An effect is **attributed** to operation O if its dispatch's `approval_digest` is admitted for O.
- An effect is **unattributed** if its dispatch is unadmitted. Workflow keys and tuples never attribute an effect.

**Counting.**
- An effect **counts as committed** if its collapsed path contains `committed` or `reversed`.
- It is **commit_unobserved** if it contains `reversed` but no `committed` observation exists.
- It **ends pending** if the last collapsed state is `pending`.

**Epoch.** `open` without `epoch_close`, otherwise `closed`. **Closure is contradicted** if any observation
sequenced after `epoch_close` either names an `effect_id` with no earlier observation, or has a `status` different
from that effect's last status before closure.

**Late effect.** A counting-committed attributed effect whose first `committed` or `reversed` observation is
sequenced after `epoch_close`.

For each operation (sorted by `operation_id`), with *C* the number of attributed counting-committed effects, the
first matching row decides:

| # | Condition | Outcome | Reason |
|---|---|---|---|
| 1 | *C* ≥ 2 | CONTRADICTED | `multiple_committed_effects` |
| 2 | closure contradicted | NOT_ESTABLISHED | `closure_contradicted` |
| 3 | epoch open | NOT_ESTABLISHED | `epoch_open` |
| 4 | `effect_coverage` incomplete | NOT_ESTABLISHED | `incomplete_effect_coverage` |
| 5 | any unattributed effect counts as committed or ends pending | NOT_ESTABLISHED | `unattributed_effect_in_scope` |
| 6 | an attributed effect of this operation ends pending | NOT_ESTABLISHED | `pending_at_closure` |
| 7 | otherwise | ESTABLISHED | `at_most_one_committed` |

## Report (schema `refund.c2-report.v0`; exit 0, stderr empty)

**Root.** The root object has exactly these keys:

| Key | Value |
|---|---|
| `schema` | `"refund.c2-report.v0"` |
| `case_id` | from the packet |
| `packet_sha256` | SHA256 of the original bytes |
| `source_class` | from the packet |
| `custody` | from the packet |
| `evidence_basis` | `"fixture_declared"` |
| `effect_coverage` | from the packet |
| `epoch` | `"open"` or `"closed"` |
| `closure_contradicted` | bool; `false` when open |
| `unattributed_blocking_effect_ids` | sorted |
| `operations` | one row per operation |
| `non_claims` | the fixed list below |

**Operation row.** Exactly these keys:

| Key | Value |
|---|---|
| `operation_id` | the operation |
| `result` | `ESTABLISHED`, `CONTRADICTED` or `NOT_ESTABLISHED` |
| `reason` | from the table above |
| `committed_effect_ids` | sorted |
| `late_effect_ids` | sorted |
| `commit_unobserved_effect_ids` | sorted |
| `reversed_effect_ids` | sorted attributed effects whose collapsed path contains `reversed` (the reviewed DESIGN reversed-effect diagnostic) |
| `workflow_keys` | sorted unique non-null keys of the operation's admitted dispatches |
| `key_hint_effect_ids` | sorted unattributed effects whose dispatch `workflow_key` is in `workflow_keys`; a diagnostic hint only |

**`non_claims`** is exactly, in this order: `no-live-execution`, `no-provider-semantics`, `no-exactly-once`,
`no-refund-safety`, `no-custody-authentication`, `no-runtime-coverage-proof`, `no-adequacy-score`.

## Frozen oracle

- `fixtures/` holds 20 semantic packets; `expectations.json` holds the literal root fields (`epoch`,
  `closure_contradicted`, `unattributed_blocking_effect_ids`) and the full `operations` array for each.
- `invalid/` holds 29 refusal cases, among them nine numeric and depth boundary cases built byte-exactly from
  SINGLE-COMMIT; `refusal-expectations.json` gives the category for each.
- `fixtures.sha256.json` binds all input bytes.

The expectation author is also a future builder. Expectations are hand-reasoned, not blind; they require an
independent non-building review before any reader code exists.
