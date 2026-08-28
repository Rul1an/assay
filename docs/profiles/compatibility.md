# Profile compatibility (canonical)

This document is the product-level policy for Assay evidence profiles. Profile
specs (`privileged-mcp-action/v0`, `privileged-mcp-action/v1`, and successors)
link here rather than restating a second compatibility rule.

## Selected interpreter versus input identity

`assay evidence verify-privileged-mcp-action` selects an interpreter with
`--profile-version` (default `v0` when omitted). The report member `profile` is
that **selected interpreter**. It is never detected from bundle bytes and is
never carried input identity.

Frozen `privileged-mcp-action/v0` and `privileged-mcp-action/v1` bundles carry
**no profile id**. For those interpreters the report MUST also carry:

| Member | Meaning |
| --- | --- |
| `profile_selection` | `default` if `--profile-version` was omitted; `explicit` if it was passed |
| `input_profile` | JSON `null` |
| `input_profile_status` | `undeclared_legacy` |

These three members are report-shape requirements outside the corpus comparison
surface; corpus vectors do not discriminate them.

Consumers MUST NOT infer producer profile, producer release, migration, rollback,
or profile provenance from `report.profile`. Changing `--profile-version` MAY
change the selected interpreter and MUST NOT create evidence of input profile
identity.

A future content-bound input profile requires a **new** profile version. It MUST
NOT be retrofitted into frozen v0/v1.

## Compatibility classes

- **Canonical bytes.** A profile version names the records it recognizes. Additive
  optional records that stay outside a selected profile's namespace are ignored
  after bundle integrity. An unrecognized **in-namespace payload schema** is
  invalid (fail closed). That exact schema string is retained on the finding as
  `observed_schema`, not only in prose. An in-namespace envelope type whose
  payload declares no schema is also invalid, but `observed_schema` is omitted
  so the envelope type is not published as a payload schema.
- **Claim meaning.** v0 and v1 share the v0 claim matrix and report schema
  `assay.privileged_mcp_action.verify.report.v0`. Selecting v1 does not change
  claim vocabulary.
- **Projections.** Report `profile` is a projection of the selected interpreter.
  Projection MUST NOT be read as a support window, upgrade path, or producer
  identity.
- **Unknown / newer schema.** Unknown in-namespace schema stays exit 2, claims
  absent. The verifier does not upgrade an unknown schema into a verified claim.

## Deprecation and support

`CHANGELOG.md` is the single announcement surface. This policy does not promise
a rolling pair of releases, and it does not promise support by date.

A released reader MUST be announced deprecated in `CHANGELOG.md` before the
release that stops accepting it. Announcement and removal MUST NOT be in the same release.
Between those two releases, acceptance is supported only where a release test
exercises that reader. Unexercised historical readers are unsupported, not promised.

Upgrade and rollback remain [#2487](https://github.com/Rul1an/assay/issues/2487).

Released 5.4 admits `privileged-mcp-action/v0` and `privileged-mcp-action/v1`
by explicit `--profile-version` (default `v0`) as reader admission only. That
admission is not input identity, producer identity, producer release,
migration, upgrade, rollback, or autodetect.

## Migration

Migration that changes claim meaning, closed vocabularies, or required identity
MUST produce a new profile version rather than rewrite bytes under an existing
identifier. This slice does **not** ship an autodetect or automatic migration
engine.

## Importer

`assay evidence privileged-mcp-action` is byte-faithful and profile-agnostic. It
does not select a profile, does not autodetect one, and does not stamp a profile
id onto the bundle. Verification owns profile semantics.
