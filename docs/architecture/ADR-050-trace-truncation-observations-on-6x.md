# ADR-050: Truncation observations ride next to the trace rows on 6.x

- Status: Proposed
- Date: 2026-09-10
- Issue: [#2787](https://github.com/Rul1an/assay/issues/2787), parent
  [#2782](https://github.com/Rul1an/assay/issues/2782)
- Related: [#2140](https://github.com/Rul1an/assay/issues/2140) (`#[non_exhaustive]` scope),
  [#2805](https://github.com/Rul1an/assay/pull/2805) (step upsert coherence),
  [#2886](https://github.com/Rul1an/assay/pull/2886) (one truncation ceiling function)
- Source baseline: `a64ca73c0c21e43b8e8fc1a7559227f9f02f54af`

This ADR records a design. Nothing in it is implemented, and line references are to the source
baseline above.

## Context

`StreamUpgrader` truncates every string above 4096 bytes through one ceiling function
(`crates/assay-core/src/trace/truncation.rs`, shared since #2886). For `Step` and `ToolCall` it
appends the returned `TruncationMeta` values to `truncations`. For `EpisodeStart` it discards both
results (`trace/upgrader.rs:65-68`): `EpisodeStart` has no field to hold them, and the `episodes`
table has no column for them (`storage/schema.rs:67-76`). An overlong prompt is shortened with no
structured record of the loss.

Two facts constrain the fix.

1. **`truncations` cannot say "measured and clean".** It is a defaulted `Vec`. Producers that never
   ran the truncator (OTel ingest, MCP import, any external constructor) serialize `[]`. A
   historical explicit `[]` and an omitted key both mean "no loss reported", not "a scan ran and
   found nothing". This is the 2026-09-04 convergence rule on #2787.
2. **A field on the event structs is a major.** `EpisodeStart`, `StepEntry` and `ToolCallEntry`
   are public structs with all-public fields and no `#[non_exhaustive]` in published `assay-core`
   (`trace/schema.rs:37-101`). A new field breaks every external struct literal
   (`constructible_struct_adds_field`); `serde(default)` does not help, because it governs
   deserialization, not construction. Marking them `#[non_exhaustive]` breaks the same literals
   (`struct_marked_non_exhaustive`). `assay-core` is on the Wave 0 semver allowlist
   ([WAVE0-GATES.md](../contributing/WAVE0-GATES.md)), checked against the latest release tag
   (`v6.1.2`), so either change fails that gate. Five integration tests in
   `crates/assay-core/tests/` build these structs as literals, standing in for external callers:
   `assertion_companion_cover.rs`, `assertions_smoke.rs`, `assertions_vacuity.rs`,
   `storage_smoke.rs`, `store_step_upsert_coherence.rs`. No 7.0 is open.

## Decision

### 1. Ship on 6.x through new public items only

This work adds no field to `EpisodeStart`, `StepEntry` or `ToolCallEntry`, adds no variant to
`TraceEvent`, and marks none of them `#[non_exhaustive]`. The carrier is a new observation record,
its reader and its storage, all additive. The implementation PR must show the
`Wave 0 semver checks (public crates)` job passing for `assay-core` with no major required.

### 2. The `#[non_exhaustive]` disagreement, resolved for this work

The #2787 body asked for `#[non_exhaustive]` on the four V2 event structs and `TraceEvent`, so
that later metadata would not cost another major. #2140 decided the opposite for existing types:
serde DTOs fall under the passive-data exemption to the API guidelines' C-STRUCT-PRIVATE, struct
literals are how evidence gets built, and growing such a type is an argued major.

For this work #2140 holds, for three reasons.

- Retrofitting the attribute is itself the major this route exists to avoid, so it cannot ride
  6.x.
- The growth #2787 wanted to protect goes into the new record instead. Later provenance about a
  trace event's bytes becomes a new member or version of the observation record, so the event
  structs have no pending growth to protect.
- `TraceEvent` is matched exhaustively by its consumers (`storage/store_trace.rs:14-19` and
  `:33-38`, `trace/upgrader.rs:64-98`). A new event kind should force each of them to decide. That
  is the enforcement argument #2140 made for `ClaimSupport`.

New types follow the Cargo Book's timing advice instead, as
[ADR-049](ADR-049-attestation-extent.md) did for its extent types: the observation record and the
observed-event wrapper are `#[non_exhaustive]` from introduction, with constructors. They are
expected to grow and have no users yet to break. The reading enum (section 5) stays exhaustive,
because a new reading must force consumers to decide rather than fall into a wildcard arm. Whether
a future major folds the record into the event structs is not decided here.

### 3. Wire shape

`truncations` keeps its meaning: loss reported by any producer, never evidence that a scan ran.

A new optional top-level key `observations` sits on the event's JSONL line, beside the existing
fields and outside caller-controlled `meta`. It is a list, and each entry is one stage's report on
the bytes it emitted:

| Member | Meaning |
|---|---|
| `v` | Observation record version. `1` for this design. |
| `stage` | Name of the component that scanned, e.g. `assay.trace.upgrader`. |
| `ceiling` | Byte ceiling the stage applied (4096 today). |
| `scope` | RFC 6901 pointers the stage scanned, e.g. `["/content", "/meta"]`. |
| `losses` | `TruncationMeta` entries the stage produced inside `scope`. |

`v` is the one member beyond the four the #2787 decision named. The reader ceiling (section 5)
needs it to tell a record it understands from one it does not.

```json
{"type":"step","episode_id":"e1","step_id":"s1","idx":0,"timestamp":1,"kind":"llm_completion",
 "name":"model","content":"...[TRUNCATED]","content_sha256":"<pre-truncation digest>",
 "truncations":[{"field":"/content","original_len":4595,"kept_len":4096,"sha256":"<d>","strategy":"head"}],
 "meta":null,
 "observations":[{"v":1,"stage":"assay.trace.upgrader","ceiling":4096,"scope":["/content","/meta"],
   "losses":[{"field":"/content","original_len":4595,"kept_len":4096,"sha256":"<d>","strategy":"head"}]}]}
```

The rules:

1. **Clean needs a present observation.** Only a present observation with empty `losses` is
   measured-clean, and only for its `scope`. Per field, the same rule restricted to that field
   applies: a trusted observation whose scope covers the field and whose `losses` hold nothing at
   or under it. This narrows the rule to one field; it does not relax it.
2. **Absent means unmeasured.** A missing key or an empty list is unmeasured. A historical explicit
   `truncations: []` is not an observation and never reads clean.
3. **Parity.** For `Step` and `ToolCall`, every entry in an observation's `losses` must equal an
   entry in the event's `truncations`. The producer meets this by construction: the upgrader
   builds both from the one `Vec` the truncation helper returns. A reader that finds an observation
   loss with no matching `truncations` entry treats that observation as absent for the clean
   reading, and still counts its losses as reported loss. `EpisodeStart` has no `truncations`, so
   its observation's `losses` are its only loss record and parity is vacuous there.
4. **Reported loss dominates.** A field with any loss at or under it, in `truncations` or in any
   observation, reads lossy whatever an observation says. A historical loss therefore survives a
   later clean rerun: the rerun appends its own observation and removes nothing.
5. **Trust.** `stage` must name a component in the trusted set that the verifying caller passes to
   the reader. Nothing is trusted by default. An observation from a stage outside that set,
   including an empty name, counts as absent. Trust is needed to raise unmeasured to clean, never
   to report loss.

The upgrader stage's scope is `["/input", "/meta"]` for `episode_start`, `["/content", "/meta"]`
for `step` and `["/args", "/result"]` for `tool_call`. A loss's `sha256` covers the bytes the stage
received at that field before shortening them, the same byte stage as the `content_sha256` and
`args_sha256` it already recomputes (`trace/upgrader.rs:73`, `:83-86`).

Producers. `StreamUpgrader` makes one internal pass that yields an event with its observation. The
existing `Iterator<Item = serde_json::Result<TraceEvent>>` stays as the projection without
observations; an additive method yields the observed event. `trace ingest` switches to that method
for both its JSONL and SQLite outputs, and keeps observations already present on an input line.
OTel ingest and MCP import run no truncator, so they emit no observation and read unmeasured.

### 4. Storage

**JSONL.** The observation is on the same line as the bytes it describes, so one write carries
both. There is no sidecar and no second file to publish or pair. A torn final line fails to parse,
as it does today.

**SQLite.** One new table, and no change to existing tables or row types:

```sql
CREATE TABLE IF NOT EXISTS trace_observations (
    target_kind    TEXT    NOT NULL, -- 'episode_start' | 'step' | 'tool_call'
    target_key     TEXT    NOT NULL, -- episodes.id | steps.id | JSON [step_id, call_index]
    ordinal        INTEGER NOT NULL, -- position in the event's observations list
    record_version INTEGER NOT NULL,
    stage          TEXT    NOT NULL,
    ceiling        INTEGER NOT NULL,
    scope_json     TEXT    NOT NULL,
    losses_json    TEXT    NOT NULL,
    bound_sha256   TEXT    NOT NULL, -- digest of the row values this observation describes
    PRIMARY KEY (target_kind, target_key, ordinal)
);
```

Names are indicative; the semantics below are the decision.

- **Atomic with the row.** A new store method writes each event's row and its observations in the
  transaction the batch already opens (`storage/store_trace.rs:24-42`). Where the row is rewritten
  (episodes and steps, last write wins), the identity's observations are replaced in the same
  transaction. Tool calls keep first-write-wins (`ON CONFLICT ... DO NOTHING`, `:154`): the
  observation is written only when the row insert took effect, and otherwise the retained row keeps
  its original observations. In the new build, the existing `insert_event` and `insert_batch` also
  delete the observations of any row they rewrite, in the same transaction.
- **Bound to the row's bytes.** A binary built before this change can write to a migrated database.
  It rewrites episode and step rows without knowing this table exists, so an observation could come
  to describe bytes the row no longer holds. Each observation row therefore stores
  `bound_sha256`: SHA-256 over the JSON array of the bound column values as stored (episodes:
  `[prompt, meta_json]`; steps: `[content, meta_json]`; tool calls: `[args, result]`). One function
  computes it at write and at read. The reader recomputes it from the current row, and a mismatch
  makes the observation absent, losses included, because it describes other bytes. The check sits
  with the reader, where the claim is made, so it covers older binaries, the first-write-wins path
  and direct edits to the row that do not recompute `bound_sha256`. The digest is keyless by
  design, so a writer that recomputes it is not caught. Reported loss for steps and tool calls is
  still read from the row's `truncations_json`, which #2805 keeps coherent with `content`. A
  binary older than #2805 can leave it stale; with the observation already dropped by the binding
  check, that yields a spurious loss or unmeasured, never clean.
- **Pointer map.** SQLite readings exist for `episodes.prompt` (`/input/prompt`),
  `episodes.meta_json` (`/meta`), `steps.content` (`/content`), `steps.meta_json` (`/meta`),
  `tool_calls.args` (`/args`) and `tool_calls.result` (`/result`). The `episodes` table stores only
  the prompt of `input`, so other `/input/...` fields have no SQLite reading.
- **Migration.** The DDL joins `storage::schema::DDL` as `CREATE TABLE IF NOT EXISTS`, with no
  `ALTER`. `DDL` is a public const, and its value changes by one additive statement.
- **Partial ingest.** `ingest_into_store` commits every 1000 events (`trace/ingest.rs:57-71`). Row
  atomicity is all the readings need, because each committed row carries the observations that
  describe it. Whether a whole file arrived is a different property and is not claimed here.

### 5. Readings and the reader ceiling

A reading for one pointer is `Lossy`, `MeasuredClean { stage, ceiling }` or `Unmeasured`. One
function computes it for both media; the SQLite reader applies the binding check and then calls
that function.

The reader ceiling is the strongest reading a record can yield:

| Record | Strongest reading |
|---|---|
| Pre-carrier line or row, including explicit `truncations: []` | `Unmeasured` (or `Lossy` if loss is reported) |
| OTel ingest, MCP import, externally constructed events | `Unmeasured` (or `Lossy`) |
| Observation with `v` above what the reader supports | Observation absent |
| Malformed `observations` value | Observation absent; the event still reads |
| Stage outside the trusted set, or empty | Observation absent |
| SQLite `bound_sha256` mismatch | Observation absent, losses included |
| Parity failure | Absent for the clean reading; its losses still count |

An unknown version or malformed value reads as absent rather than refused. This departs on purpose
from [ADR-044](ADR-044-attestation-subject-is-the-artifact.md), which refuses unknown majors: here
a refusal would make a trace unreadable because of an optional carrier, while absence yields the
weakest reading.

Readers built before this change cannot misread the carrier. On JSONL they deserialize the event
structs, which do not deny unknown fields (`trace/schema.rs`; `providers/trace_next/v2.rs` reads
through `from_value` the same way), so they ignore `observations` and read `truncations` as today.
They have no clean reading to get wrong. An older tool that reserializes a line drops the key,
and downstream reads that as unmeasured. On SQLite, older binaries never read the new table, their
`CREATE TABLE IF NOT EXISTS` DDL runs unchanged against it, and their rewrites are caught by the
binding check.

## Required tests for the implementation PR

Each must-bite test names the production mutation it kills. The mutation must turn its test red
while a positive control on the same code line stays green.

Must-bite:

1. **Absent observation read as clean.** A pre-carrier V2 line with no `observations` key, a line
   with `truncations: []`, and a SQLite step row with `truncations_json = '[]'` and no observation
   row all read `Unmeasured`. Mutation: the reader returns `MeasuredClean` when no observation
   covers the field. Control: the same line with a trusted empty-loss observation reads
   `MeasuredClean`.
2. **Empty list read as clean.** `"observations": []` reads `Unmeasured`. Mutation: an empty list is
   treated as one empty-loss observation.
3. **Loss dropped from observations but present in `truncations`.** A step with `truncations` =
   `[X at /content]` and a trusted observation whose `losses` omit X reads `Lossy` at `/content`.
   Mutation: the reader consults observation losses only. Producer half: the upgrader's observation
   losses equal the entries it appended to `truncations`; mutation: build them from a second call
   or an empty `Vec`.
4. **Unnamed stage accepted.** An empty-loss observation from a stage outside the trusted set, and
   one with `stage: ""`, read `Unmeasured`; an empty trusted set yields no `MeasuredClean` anywhere.
   Mutation: the trust check is skipped.

Also required:

5. **EpisodeStart loss retained.** A 4,595-byte ASCII prompt yields one loss at `/input/prompt`
   with `original_len == 4595`, `kept_len` equal to the emitted byte length and `sha256` over the
   submitted bytes; a nested meta value yields `/meta/a/b`. Both survive JSONL and SQLite round
   trips. Mutation: restore the discard at `trace/upgrader.rs:67-68`.
6. **Bypass producers.** OTel ingest and MCP import emit no observation and read `Unmeasured`.
   Mutation: attach an upgrader observation.
7. **Binding.** After an observed step is rewritten by the pre-change step upsert SQL executed
   directly, the step does not read `MeasuredClean`. Mutation: skip the binding check. After an
   observed episode is rewritten the same way by the pre-change episode upsert SQL with a changed
   prompt, `/input/prompt` does not read `MeasuredClean`. Mutation: skip the binding check for
   episodes only. A second observed tool call with the same identity and different `args` leaves
   the retained row's reading equal to the first write's.
8. **Atomicity.** A failing observation insert rolls back the row of the same event. Mutation:
   commit observations in a separate transaction.
9. **Older readers.** A line carrying `observations` deserializes to the same `TraceEvent` as the
   line without it, through `StreamUpgrader` and through `trace_next`. A database holding the new
   table accepts the pre-change DDL and inserts.
10. **Rerun pair from 2026-09-04.** A rerun over a legacy nonempty loss leaves `truncations`
    unchanged and reads `Lossy`. A rerun over `truncations: []` leaves it empty and reads
    `MeasuredClean` only through the new observation.
11. **Unknown version and malformed value** read `Unmeasured`, and the event still reads.
12. **Semver.** The Wave 0 semver job reports no major for `assay-core`.

Must-bite, added in review:

13. **Per-field clean.** A step with a short `/content` and one loss at `/meta/a`, held in both
    `truncations` and a trusted observation with scope `["/content", "/meta"]`, reads `Lossy` at
    `/meta` and `/meta/a` and `MeasuredClean` at `/content`. A reading at `/meta2` is
    `Unmeasured`, because the `/meta` scope covers whole segments only. Mutations: observation-level
    clean, where any loss vetoes the whole observation (`/content` turns red, the `Lossy` readings
    are the control); string-prefix scope matching (`/meta2` turns red).
14. **Extra observation loss.** The test 13 step with the `/meta/a` loss removed from
    `truncations` reads `Lossy` at `/meta/a`, because the observation's loss still counts, and
    `Unmeasured` at `/content`, because parity fails and the observation is absent for the clean
    reading. Mutation: the reader ignores observation losses that have no `truncations` match,
    whether by not counting them or by skipping the parity branch. Control: the test 13 record
    reads `MeasuredClean` at `/content`.

## Non-claims

- A stage name is not authentication. A clean reading is a self-assertion by whoever wrote the
  record, accepted because the caller chose to trust that name on that channel. No signature is
  added.
- A clean reading is stage-local: the named stage did not shorten values in its scope at its
  ceiling. An exporter, SDK or host upstream may already have truncated them. This is not
  end-to-end completeness.
- A loss digest neither recovers lost bytes nor authenticates a producer. The in-band
  `...[TRUNCATED]` sentinel stays unauthenticated.
- JSONL carries no binding digest. A later edit that changes a value but keeps the line's
  `observations` is not detected; whoever rewrites the line is the author of both.
- For `EpisodeStart` the observation is the only loss record. A producer that drops a loss from it
  is caught by the producer tests (test 5), not by the reader, which has nothing to compare with.
- Pre-carrier records do not become decidable; they stay unmeasured.
- File-level ingest completeness is not addressed.
- No consumer changes its verdict in this work. `trace verify`, coverage and policy decisions read
  what they read today; wiring readings into a verdict is separate work. Before any reading feeds
  a verdict, that work must decide between a binding digest over the scoped JSONL values and
  taking evidentiary clean readings from SQLite only, and it inherits the self-assertion that
  trusting a stage by name concedes.

## Alternatives considered

| Option | Why not |
|---|---|
| v7 major: `Option<Vec<TruncationMeta>>` on the three structs, plus `#[non_exhaustive]` | No 7.0 is open, so it would park the only P1 trace bug indefinitely. It is also wrong on its own terms: `Option<Vec>` would read a historical explicit `[]` as `Some([])`, which is measured-clean (the 2026-09-04 rule). |
| A column on `episodes` (`truncations_json` or `observations_json`) | Covers episodes only; steps and tool calls would need matching columns. A column goes stale under an older writer's upsert just as a table row does, so it needs the same binding. An `episodes.truncations_json` would be a reported-loss column with no wire counterpart, since `EpisodeStart` has no `truncations`, and exposing it through `EpisodeRow` would add a field to a published struct. One table carries all three kinds with a version per row. |
| `#[non_exhaustive]` on the existing structs and `TraceEvent` | Itself a major; see section 2. |
| A sidecar file next to the JSONL (the 2026-09-05 comment on #2782) | Two files cannot be renamed atomically, so it needs pairing and temp-then-rename, and copying the `.jsonl` alone loses it. The same-line key takes its atomicity from the line. That proposal's SQLite half, one transaction for rows and provenance, is kept. |
| A new `TraceEvent` variant for observation lines | A variant on an exhaustive public enum is a major, and older readers fail on the unknown tag (`StreamUpgrader` returns `Err`). |
| Carry observations in `meta` | `meta` is caller-controlled; it would mix Assay provenance with caller payload (#2782). |
| SQLite triggers that delete observations on row update | Covers every writer, but lives in schema objects that a copy or rebuild can drop, and acts on the write side. The digest check sits with the reader that makes the claim. |

## Consequences

- `EpisodeStart` loss becomes recordable on 6.x without a major.
- A reader must be given a trusted stage set; with none, nothing reads clean. That is intended.
- Step and tool-call loss is carried twice, in `truncations` and in observation `losses`. Rule 3
  and the single producer `Vec` keep them in step.
- The observation table grows by one row per stage per observed event.
