# Named CLI JSON Schemas Design

Date: 2026-08-08
Issue: [#2159](https://github.com/Rul1an/assay/issues/2159)

## Problem

The golden-path measurements found three different JSON documents behind two
commands:

| command path | current top-level keys |
|---|---|
| `assay validate --format json` | `command`, `diagnostics`, `exit_code`, `ok`, `schema_version`, `suggested_actions`, `suggested_patches`, `summary`, `tool` |
| successful `assay run --format json` | `results`, `run_id`, `suite` |
| early-failure `assay run --format json` | `exit_code`, `message`, `next_step`, `provenance`, `reason_code`, `reason_code_version`, `schema_version`, `seeds` |

The two run paths have no top-level key in common. An integer
`schema_version: 1` does not identify which document a caller holds, and the
successful run document has no schema identity at all.

## Decision

Add a top-level string `schema` discriminator and retain an integer
`schema_version` for these three documents:

- `assay.validate_report.v1` for validate JSON;
- `assay.run_report.v1` for the detailed successful run-results report;
- `assay.run_summary.v1` for `summary.json` and early-failure run stdout.

The identifiers name documents, not commands. A caller branches on `schema`
before interpreting document-specific fields. `schema_version` remains `1`
and is scoped by that name.

The broader repository convention is intentionally separate. Coverage,
session-state-window, and soak reports currently put string identities in
`schema_version`;
[#2167](https://github.com/Rul1an/assay/issues/2167) owns inventory and
migration of that cross-command convention. It does not block naming the
three documents required by #2154.

## Compatibility

Do not add a field to public `assay_core::report::summary::Summary`. That
struct is constructible downstream, so adding a field would be a semver-major
Rust API change.

Instead, add one public summary renderer in the existing writer module:

1. serialize `Summary` to a `serde_json::Value`;
2. insert `schema: assay.run_summary.v1` into the top-level object;
3. pretty-print the value;
4. call that renderer from both `write_summary` and early-failure stdout.

Old summaries remain deserializable because `Summary` continues to accept
unknown fields and does not require the discriminator. Replaying an old
summary reads the old shape, then the shared writer adds the current identity.
Replaying a new summary drops the unknown field during deserialization and
adds the same value again, so the round trip is idempotent.

The renderer preserves existing key order before appending `schema` because
the active serde_json graph enables `preserve_order`. A unit test pins the
first key as `schema_version`, so removal of that transitive feature cannot
silently reorder every summary artifact.

## Single Sources

- `validate.rs` defines the validate schema id beside its sole JSON builder.
- `report/json.rs` defines the run-results schema id and version beside the
  one renderer used by stdout and its file writer.
- `summary/writer.rs` defines the summary schema id beside the one renderer
  used by summary files and early-failure stdout.

No caller restates these string literals.

## Render Safety

This change preserves current `Summary` bytes except for the additive schema
field. It does not silently add or remove sanitization. The current asymmetry
between summary rendering and the render-safety walk used by run results is
tracked in [#2168](https://github.com/Rul1an/assay/issues/2168), which must
first establish whether untrusted bytes can reach `Summary.message` or
`next_step`.

The new run-report keys pass through the existing JSON safety walk. They are
Assay-owned constants and are not part of its untrusted-field vocabulary.

## Tests

A binary-level CLI contract test drives:

1. validate success and failure, both named `assay.validate_report.v1`;
2. run success, named `assay.run_report.v1`;
3. run early failure, named `assay.run_summary.v1`;
4. the success-path `summary.json`, also named `assay.run_summary.v1`;
5. distinct identities across all three document contracts.

Core tests pin:

- the run-results renderer's schema id and integer version;
- the summary renderer's schema id;
- the existing summary key order before the appended field;
- old direct-serialized summaries and new rendered summaries both
  deserializing into `Summary`;
- a render-read-render round trip retaining exactly one schema field.

Targeted mutations must prove that changing each of the three schema ids makes
its owning test fail.

## Documentation

- Add `schema` to the normative `summary.json` table and examples in
  `SPEC-PR-Gate-Outputs-v1.md` as an additive v1 field.
- Document validate's schema identity in its CLI reference.
- Document the run-success, run-failure, and summary-artifact identities in
  the run reference.
- Add the summary identity to the run-output AI context.

## Non-Goals

- No shared envelope or payload wrapper.
- No removal or repurposing of `schema_version`.
- No identity for the separately written `run.json` artifact; #2167 owns that.
- No migration of coverage, session-state-window, or soak reports; #2167 owns
  that.
- No provenance expansion.
- No summary render-safety behavior change; #2168 owns that decision.
- No change to exit codes, reason codes, result semantics, or output channels.
