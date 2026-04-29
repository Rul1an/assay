# PLAN — P52-P56 Assay Product Surface Consolidation Program (Q2 2026)

- **Date:** 2026-04-29
- **Owner:** Assay maintainers
- **Status:** planning program
- **Scope:** Consolidate the post-v3.8.0 Assay product surface before opening
  another evidence family or upstream wedge.

## 1. Why this program exists

Assay now has enough strong lanes:

- agent runtime signals and MCP governance evidence,
- canonical Evidence Bundles,
- Trust Basis and Trust Card artifacts,
- three claim-visible external receipt families,
- a machine-readable receipt schema registry,
- and Assay Harness compatibility proof against the released v3.8.0 line.

The next risk is not lack of capability. The next risk is product legibility.
A new reader should not have to reconstruct the difference between
policy-as-code, evidence bundles, receipt imports, Trust Basis claims, Trust
Cards, Harness recipes, SARIF projections, and compliance packs from history.

This program turns the existing lanes into one coherent product surface:

```text
understandable -> assertable -> schema-validatable -> reviewable -> bound to policy/tool identity
```

It deliberately does not add a new receipt family, external wedge, dashboard,
or integration claim.

## 2. Program invariants

All slices in this program must preserve these rules:

- Assay remains an evidence and trust compiler, not an eval runner,
  observability dashboard, BOM viewer, or generic policy platform.
- Trust Basis JSON remains canonical for claim classification.
- Trust Card Markdown/HTML and CI outputs are projections only.
- Consumers key Trust Basis claims by stable `claim.id`, never row position or
  row count.
- Receipt-family docs must keep included fields, excluded fields, and
  `does_not_claim` semantics visible.
- Harness remains outside Assay receipt payload semantics.
- No slice may imply official support, partnership, or endorsement from
  Promptfoo, OpenFeature, CycloneDX, Mastra, or any runtime provider.

## 3. P52 — Product Truth Sync

### Goal

Make the public product surface tell one story everywhere:

```text
Assay compiles agent runtime signals and selected external outcomes into
verifiable evidence and bounded Trust Basis claims.
```

### Work

- Update top-level product language across README, docs index, CLI help, and
  quickstart surfaces.
- Demote older "Policy-as-Code for AI Agents" language from the primary
  product identity to one capability inside the broader trust compiler story.
- Keep compliance packs visible but not first-positioned as the product wedge.
- Add or refresh a short "What Assay is / is not" reference page.
- Separate Assay core from Assay Harness in public docs:
  - Assay owns evidence, receipts, bundle verification, Trust Basis, Trust Card,
    schema registry, and claim semantics.
  - Assay Harness owns orchestration, regression gating, and CI projections over
    Assay artifacts.
- Link the receipt family matrix as a first-class public artifact from the
  docs surfaces that introduce external receipts.

### Non-goals

- No roadmap rewrite from scratch.
- No new runtime behavior.
- No new Trust Basis claims.
- No new receipt families.
- No new compliance or partnership claims.

### Acceptance

- `assay --help`, README, docs index, and first-run docs use compatible product
  language.
- A new reader can identify the canonical chain:

```text
runtime/import signal -> evidence bundle -> Trust Basis -> Trust Card / diff
```

- The receipt family matrix remains linked from public receipt docs.

## 4. P53 — Trust Basis Assert

### Goal

Make a single canonical `trust-basis.json` directly assertable in CI without
requiring Harness or family-specific policy logic.

### Command shape

```bash
assay trust-basis assert \
  --input trust-basis.json \
  --require external_eval_receipt_boundary_visible=verified
```

### Work

- Add a small `trust-basis assert` command.
- Accept one or more `--require <claim-id>=<level>` predicates.
- Key only by `claim.id`.
- Validate that input is a canonical Trust Basis artifact.
- Emit machine-readable JSON.
- Emit a short text summary for humans.
- Preserve exit codes:
  - `0`: all requirements satisfied
  - `1`: at least one requirement mismatch
  - `2+`: input, config, or runtime error

### Non-goals

- No baseline/candidate comparison.
- No Trust Basis diff replacement.
- No Harness behavior.
- No Promptfoo/OpenFeature/CycloneDX-specific policy.
- No new Trust Basis claims.

### Acceptance

- Any existing claim can be asserted by stable `claim.id`.
- Missing claims fail as policy mismatch, not success.
- Unknown levels or malformed requirements fail as config/input errors.
- JSON output includes required claim id, expected level, actual level, and
  pass/fail status.

## 5. P55 — Receipt Schema CLI

### Goal

Turn the v3.8.0 receipt schema registry into a discoverable product surface.

### Command shape

```bash
assay evidence schema list
assay evidence schema show promptfoo.assertion-component.v1
assay evidence schema validate \
  --schema promptfoo.assertion-component.v1 \
  --input receipt.json
```

### Work

- Add schema discovery for:
  - receipt payload schemas,
  - importer input schemas where those differ from receipt payloads.
- Support Promptfoo, OpenFeature, CycloneDX ML-BOM, and Mastra schema entries.
- Keep Mastra marked as importer-only.
- Show useful metadata before raw schema content:
  - schema id,
  - family,
  - status,
  - source file,
  - short description,
  - whether the schema is receipt payload or importer input.
- Validate inputs with JSON pointer style error paths where practical.
- Keep the schema registry linked to
  `docs/reference/receipt-family-matrix.json`.

### Non-goals

- No new schemas beyond the existing v3.8.0 family set.
- No new receipt families.
- No Trust Basis classification changes.
- No Harness validation responsibility.

### Acceptance

- `list` returns all v3.8.0 receipt and importer-input schemas.
- `show` can return metadata plus raw JSON schema.
- `validate` passes for checked-in fixtures and importer-generated supported
  receipt payloads.
- `validate` fails closed for malformed payloads with actionable paths.

## 6. P54 — Static Trust Card HTML

### Goal

Make Trust Cards more reviewable without creating a hosted dashboard or a second
source of truth.

### Work

- Add HTML projection support to Trust Card generation.
- Produce a deterministic single-file artifact.
- Render the same claim rows as `trustcard.json` and `trustcard.md`.
- Show claim id, level, source, boundary, and note.
- Keep `trustcard.json` canonical; Markdown and HTML are projections.

### Non-goals

- No hosted dashboard.
- No remote assets.
- No JavaScript requirement to understand the artifact.
- No scores, badges, or second classifier.
- No claim semantics added in HTML.
- No receipt-family context expansion or raw evidence links in this slice.

### Acceptance

- HTML output is deterministic for the same Trust Basis / Trust Card input.
- JSON remains canonical; HTML is documented as projection only.
- HTML renders without network access.
- Tests prove claim ordering and level rendering match the canonical artifact.

## 7. P56a — Policy Snapshot Digest Visibility

Status: execution slice tracked in
[PLAN-P56a](./PLAN-P56A-POLICY-SNAPSHOT-DIGEST-VISIBILITY-2026q2.md).

### Goal

Make the policy snapshot that governed a decision visibly digest-bound in
evidence and review artifacts.

### Work

- Identify the current canonical policy snapshot object for MCP/runtime
  decisions.
- Surface a stable policy snapshot digest in evidence where available.
- Carry digest visibility into Trust Basis metadata or supporting evidence
  without implying policy quality.
- Document the boundary:

```text
policy snapshot digest visible != policy is correct or sufficient
```

### Non-goals

- No policy authoring redesign.
- No waiver lifecycle.
- No proof that policy is safe.
- No Sigstore/keyless layer yet.

### Acceptance

- A reviewer can see which policy snapshot governed a supported decision path.
- Digest drift is reviewable.
- Missing policy digest remains explicit rather than silently treated as safe.

## 8. P56b — Tool Definition Digest Binding

### Goal

Bind supported tool decisions to the tool definition surface that was reviewed,
without claiming the tool is safe or fully signed.

### Work

- Reuse the existing tool signing / definition canonicalization line where
  possible.
- Surface tool definition digest alongside supported decision evidence.
- Document how this relates to `x-assay-sig`, DSSE, and future transparency-log
  work.
- Keep runtime/provider-specific tool surfaces out unless there is a canonical
  bounded definition object.

### Non-goals

- No full tool signing completion.
- No transparency log requirement.
- No claim that the tool is safe.
- No provider-wide tool registry import.

### Acceptance

- A supported decision can be reviewed with:
  - policy snapshot digest,
  - tool definition digest,
  - decision evidence,
  - Trust Basis posture.
- Tool-definition drift is visible as review evidence.
- The implementation does not invent identity for runtimes that do not expose a
  bounded tool definition surface.

## 9. Preferred execution order

1. **P52** — product truth sync.
2. **P53** — Trust Basis assert.
3. **P55** — receipt schema CLI.
4. **P54** — static Trust Card HTML.
5. **P56a** — policy snapshot digest visibility.
6. **P56b** — tool definition digest binding.

P55 may run in parallel with P53 if write scopes are kept separate. P54 should
wait until the assert and schema surfaces are stable enough to avoid producing a
pretty artifact over unstable terminology. P56a and P56b should remain separate
unless implementation discovery proves the write scope is tiny.

## 10. Program test plan

Per slice:

- `cargo fmt --check`
- targeted `cargo test -p assay-evidence`
- targeted `cargo test -p assay-cli`
- `cargo clippy -p assay-evidence --all-targets -- -D warnings`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- docs link check for changed docs
- `git diff --check`

For P54 and later, add deterministic golden tests for projection output.

For P56a/P56b, add regression tests that prove missing or malformed digest
visibility does not accidentally classify as verified trust.

## 11. External posture

Do not publish a new external wedge from this program until P52, P53, and P55
are landed. If P54 lands soon after, use the HTML Trust Card as the reviewer
artifact in demos. P56a/P56b are stronger security follow-ups, not blockers for
basic external readability.

External language should stay small:

```text
Assay compiles selected agent/runtime and external evidence surfaces into
bounded, verifiable claims that can be reviewed in CI.
```

No call to action, no partnership language, and no claim that upstream systems
endorse the Assay receipt lanes.
