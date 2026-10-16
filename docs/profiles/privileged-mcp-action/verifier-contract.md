# Privileged MCP Action verifier: CLI contract

- Command: `assay evidence verify-privileged-mcp-action BUNDLE [--format json|table] [--profile-version v0|v1]`
- Status: **experimental**. This page describes what the shipped verifier does today. It adds no
  rule; the record and stage rules stay in [`v0.md`](v0.md) and [`v1.md`](v1.md), and selection
  policy stays in [`../compatibility.md`](../compatibility.md).
- Report schema: [`privileged_mcp_action_verify_report_v0.schema.json`](../../../schemas/privileged_mcp_action_verify_report_v0.schema.json)
  (see [Machine-readable schema](#machine-readable-schema)).

## Inputs

- One local evidence bundle path. The verifier is offline: it takes no key, no policy file, and no
  network input.
- One selected interpreter. Omitting `--profile-version` applies v0 and reports
  `profile_selection: "default"`; passing `v0` or `v1` reports `"explicit"`. Nothing is detected from
  the bundle, and v0/v1 bundles carry no profile id (`input_profile: null`,
  `input_profile_status: "undeclared_legacy"`).
- Any other `--profile-version` value is a command-line usage error: exit 2, a message on stderr, and
  **no report** on stdout. It never falls back to v0.

## Outcomes and exit code

An invocation that passes command-line and environment validation and reaches the verifier prints
exactly one report (`schema: "assay.privileged_mcp_action.verify.report.v0"`, for both
interpreters) in the requested format. A usage error stops before that point: exit 2, a message on
stderr, and no report. Besides an unknown `--profile-version`, this includes an invalid
`ASSAY_QUIET`, `ASSAY_COLOR` or `ASSAY_NON_INTERACTIVE` value, which the CLI rejects for every
command after argument parsing.

| Outcome | `bundle_integrity` | `verdict` | `claims` | `reason_code` | Exit |
|---|---|---|---|---|---|
| Valid, every cell `confirmed` or `incomplete` | `pass` | `valid` | present | absent | 0 |
| Valid with a `refuted` cell | `pass` | `valid` | present | absent | 0 |
| Profile violation (stages 2-3) | `pass` | `invalid` | absent | `E_EVIDENCE_PROFILE_INVALID` | 2 |
| Bundle integrity failure (stage 1) | `fail` | absent | absent | one stage-1 code below | 2 |
| Usage error (argument or environment) | no report | | | | 2 |

For every report the rule is: **exit 0 if and only if `bundle_integrity` is `pass` and `verdict`
is `valid`; otherwise 2.** A refuted claim is a result, not a failure, so consumers gate on claim cells and not
on the exit code. This command uses only 0 and 2 from the CLI exit vocabulary; it never emits 1, 3
or 4.

Stage-1 reason codes: `E_EVIDENCE_INTEGRITY` (hash or run-root mismatch), `E_EVIDENCE_CONTRACT`
(bundle layout, ids, sequence, JSON), `E_EVIDENCE_LIMIT_EXCEEDED` (a resource ceiling),
`E_EVIDENCE_PATH_REJECTED` (unsafe member path), `E_EVIDENCE_UNREADABLE` (cannot open or decode,
including a missing file or a directory). `reason_code` and `next_step` are diagnostics outside the
claim lattice and are absent on every valid report.

## Unknown input

These follow from the code; this page does not tighten any of them.

- An event is ignored after integrity only when both its envelope type and its payload `schema`
  are outside the profile namespaces.
- An in-namespace payload schema the selected interpreter does not recognize is `invalid`, with the
  exact string on `observed_schema`. Another interpreter's observation schema counts as unrecognized.
- An in-namespace event type whose payload declares no in-namespace schema (no schema, or one
  outside the namespaces) is `invalid`, without `observed_schema`.
- Inside a recognized record, only the members the profile names are read. **Additional members are
  ignored**, at top level and nested; they neither fail the record nor change a cell.
- A producer `non_claims` array must contain every required string byte-for-byte; **additional
  entries are accepted**.

## What the report is not

- **Not a signed attestation.** Stage 1 checks content hashes, the run root and the bundle id. The
  bundle is unsigned and no signature or key is checked. `bundle_integrity: "pass"` means the bytes
  are internally consistent, not that a named party produced them, and it never upgrades
  `source_class` above `producer_reported`.
  Signature verification for a separate bundle attestation is provided by
  `assay evidence verify-attestation --key`, described in the
  [CLI reference](../../reference/cli/evidence.md#verify-an-artifact-attestation) and
  [attestation contract](../../attestation/evidence-bundle/v1.md). Its explicit Ed25519 SPKI public
  PEM format and trust-policy boundary belong to that command; they are not inputs to this unsigned
  profile verifier and do not upgrade its claim cells.
- **Not a conformance-run report.** The schema at
  [`conformance/privileged-mcp-action-v0/report.schema.json`](../../../conformance/privileged-mcp-action-v0/report.schema.json)
  describes a run of an implementation over the corpus, including that run's `implementation`
  object. The per-bundle report above carries no evaluator identity: no implementation name, version
  or build. Adding one is open follow-up work on [#2497](https://github.com/Rul1an/assay/issues/2497)
  and would need its own compatibility review.
- **Not a scorer contract.** Harness scoring, aggregation and incident-package reports are separate
  surfaces and are not specified here.
- The four fixed report `non_claims` apply to every report, failures included.

## Machine-readable schema

[`schemas/privileged_mcp_action_verify_report_v0.schema.json`](../../../schemas/privileged_mcp_action_verify_report_v0.schema.json)
(JSON Schema 2020-12, experimental) describes the report above as the current verifier emits it:
the three report shapes, `input_profile` present as `null`, the four claim cells, the diagnostics and
the fixed `non_claims`.

- **Emitter closure, not an input rule.** The schema is closed, following the CLI JSON convention in
  [`CLI-JSON-IDENTITIES.md`](../../architecture/CLI-JSON-IDENTITIES.md): a compatible addition to the
  report keeps the `.v0` identity and edits the schema in the same commit. It changes nothing a bundle
  reader must accept, and stdout consumers may still read only the members they need.
- **`source_class` is `producer_reported` only**, because that is the only class this verifier emits.
  The six-value vocabulary in the conformance-run schema stays the general one, and that schema is
  unchanged.
- **Current output only.** Reports printed by earlier releases under the same identity are not
  promised to validate; for example, they lack `profile_selection`, `input_profile` and
  `input_profile_status`.

## Coverage

Outcomes are pinned by the v0 and v1 corpora
(`crates/assay-cli/tests/conformance_privileged_mcp_action.rs`,
`crates/assay-cli/tests/conformance_privileged_mcp_action_v1.rs`), selection and projection by
`crates/assay-cli/tests/profile_projection_honesty.rs`, record-level rules by the unit tests in
`crates/assay-cli/src/cli/commands/evidence/verify_privileged_mcp_action.rs`, and the report schema by
the schema tests in `crates/assay-cli/tests/conformance_privileged_mcp_action_v1.rs`.
