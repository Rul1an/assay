# ADR-023: CICD Starter Pack (Adoption Floor)

## Status

Accepted (February 2026)

## Context

Per [ADR-016](./ADR-016-Pack-Taxonomy.md) and the [ROADMAP §F](../ROADMAP.md#f-starter-packs-oss-p1), the open core requires a **compatibility floor for adoption**—a lightweight pack that gives teams first value from `assay evidence lint` with minimal config, before they graduate to compliance packs (eu-ai-act-baseline, soc2-baseline).

Challenges:
- eu-ai-act-baseline and soc2-baseline are **compliance-focused** (regulatory mapping, disclaimers); they assume prior familiarity with evidence format and lint workflow.
- New teams need a **zero-friction first signal**: "Does my CI produce valid, traceable evidence?" without regulatory framing.
- A starter pack must be **composable** with compliance packs so teams can add `--pack eu-ai-act-baseline` when they need Article 12 coverage.

## Decision

### 1. Pack identity and kind

We introduce **`cicd-starter`** as a **quality** pack:

| Field | Value | Rationale |
|-------|-------|-----------|
| `name` | `cicd-starter` | Clear, CI-focused; distinct from compliance pack names |
| `kind` | `quality` | No disclaimer; light best-practice checks; see [ADR-016](./ADR-016-Pack-Taxonomy.md) |
| `version` | `1.0.0` | Initial release |

**No disclaimer** — `kind: quality` packs do not require a legal disclaimer per SPEC-Pack-Engine-v1. The pack README SHALL include a short note: "These checks verify evidence hygiene for CI pipelines. They do not constitute compliance certification."

### 2. Rule set (minimal traceability)

Only **existing** check types from [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md): `event_count`, `event_pairs`, `event_field_present`. No new engine behaviour.

| Rule ID | Check | Severity | Evidence required |
|---------|-------|----------|-------------------|
| CICD-001 | Bundle has ≥1 event | error | ≥1 event in bundle |
| CICD-002 | Lifecycle pair present | warning | `assay.profile.started` + `assay.profile.finished` |
| CICD-003 | Correlation ID present | warning | `traceparent`, `tracestate`, or `run_id` (top-level or in data) |
| CICD-004 | Build identity present | info | `data.build_id` or `data.version` |

**Rationale:**
- CICD-001: Empty bundle = no evidence; fail fast.
- CICD-002: Exact `assay.profile.started` / `assay.profile.finished` for beginners—avoids "mysterious pass" from unrelated `*.started` events. README explains how to customize patterns for custom lifecycle types.
- CICD-003: Canonical top-level W3C fields (`traceparent`, `tracestate`, `run_id`) + `/data/*` fallback; semantically distinct from build metadata.
- CICD-004: Build identity (build_id, version) supports provenance maturity; info severity as optional signal toward SLSA/attestation readiness.

### 3. Check definitions (normative)

```yaml
# cicd-starter@1.0.0
name: cicd-starter
version: "1.0.0"
kind: quality
description: Minimal traceability for CI pipelines — compatibility floor for adoption
author: Assay Team
license: Apache-2.0

requires:
  assay_min_version: ">=2.10.0"
  evidence_schema_version: "1.0"

rules:
  - id: CICD-001
    severity: error
    description: Evidence bundle contains at least one recorded event
    help_markdown: |
      ## CICD-001: Event Presence
      The bundle must contain at least one event. An empty bundle indicates
      no evidence was captured for this run.

      **How to fix:**
      - Ensure your CI runs `assay evidence export` (or equivalent) before lint.
      - Verify the evidence pipeline emits at least one CloudEvents event.
      - Re-run: `assay evidence lint bundle.tar.gz --pack cicd-starter`
    check:
      type: event_count
      min: 1

  - id: CICD-002
    severity: warning
    description: Events include assay profile lifecycle (started/finished pair)
    help_markdown: |
      ## CICD-002: Lifecycle Traceability
      At least one `assay.profile.started` and one `assay.profile.finished` event
      should exist. These represent CI-run boundaries. If you use custom lifecycle
      types, see the pack README for pattern customization.

      **How to fix:**
      - Ensure your evidence pipeline emits profile start/finish events.
      - Assay's default export includes these when using `assay run` or trace replay.
      - Re-run: `assay evidence lint bundle.tar.gz --pack cicd-starter`
    check:
      type: event_pairs
      start_pattern: "assay.profile.started"
      finish_pattern: "assay.profile.finished"

  - id: CICD-003
    severity: warning
    description: At least one event contains correlation ID (traceparent, tracestate, run_id)
    help_markdown: |
      ## CICD-003: Correlation (W3C Trace Context)
      Events should include traceparent, tracestate, or run_id to link evidence
      to external observability (e.g. OpenTelemetry).

      **How to fix:**
      - If using OpenTelemetry: propagate traceparent into the event envelope.
      - Otherwise: set run_id (UUID or deterministic ID) for each run in event data.
      - Re-run: `assay evidence lint bundle.tar.gz --pack cicd-starter`
    check:
      type: event_field_present
      paths_any_of:
        - /traceparent
        - /tracestate
        - /run_id
        - /data/traceparent
        - /data/tracestate
        - /data/run_id

  - id: CICD-004
    severity: info
    description: At least one event contains build identity (build_id or version)
    help_markdown: |
      ## CICD-004: Build Identity
      Build identity fields support provenance maturity and help you build
      toward SLSA-style attestations.

      **How to fix:**
      - Add `build_id` or `version` to event data in your evidence pipeline.
      - Re-run: `assay evidence lint bundle.tar.gz --pack cicd-starter`
    check:
      type: event_field_present
      paths_any_of:
        - /data/build_id
        - /data/version
```

### 4. Pack layout and delivery

| Item | Specification |
|------|---------------|
| **Source location** | `packs/open/cicd-starter/` with `pack.yaml`, `README.md`, `LICENSE` (Apache-2.0) |
| **Built-in** | Yes. Add to `BUILTIN_PACKS` in `assay-evidence` so `--pack cicd-starter` works without local discovery. |
| **Vendoring (normative)** | `packs/open/*` MUST be vendored into `crates/assay-evidence/packs/*` (copy or symlink) for `include_str!` and `cargo publish`. This is a repo rule; same pattern as eu-ai-act-baseline, soc2-baseline. |

### 5. Default pack and CLI UX

**`cicd-starter` SHALL be the default pack** when no `--pack` is specified. This eliminates choice friction and aligns with PLG best practice ("run lint" without configuration).

**Header output:** When using the default, the lint output SHALL show:
```
Packs: cicd-starter@1.0.0 (default)
Hint: Use --pack eu-ai-act-baseline to add compliance mapping.
```

```bash
# Default: cicd-starter (no --pack needed)
assay evidence lint bundle.tar.gz

# Explicit pack (equivalent)
assay evidence lint bundle.tar.gz --pack cicd-starter

# Starter + compliance (graduation path)
assay evidence lint bundle.tar.gz --pack cicd-starter,eu-ai-act-baseline
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline   # overrides default
```

No collision: cicd-starter rule IDs (CICD-001 through CICD-004) are distinct from eu-ai-act (EU12-*) and soc2 (invariant.*).

### 6. Documentation

#### Pack README (normative structure)

| Section | Content |
|---------|---------|
| **Rules table** | Rule ID \| Check \| Severity \| Evidence required |
| **Quickstart** | "First signal in <5 min" with copy/paste GitHub Action snippet |
| **Action snippet** | **Pinned to commit SHA** (supply-chain hardening); default `fail_on: error`; note that **warnings won't fail CI by default—use `--fail-on warning` to enforce** |
| **Advanced** | Reusable workflow (`workflow_call`) with inputs: `bundle_path`, `pack`, `fail_on`; link to GH docs |
| **Next steps** | Buttons/links: `--pack soc2-baseline`, `--pack eu-ai-act-baseline` for graduation |
| **Provenance** | These checks help build toward **provenance maturity** (SLSA, attestations); prepares evidence hygiene for attestation workflows |
| **CICD-002 customization** | How to extend patterns if using custom lifecycle event types |

#### Severity and exit-code UX

- Warnings (CICD-002, CICD-003) do **not** fail CI by default.
- README SHALL state: "Set `--fail-on warning` to enforce warnings in CI."

## Consequences

### Positive

- Teams get first value from `assay evidence lint` without regulatory context.
- Composable with compliance packs; clear graduation path.
- No new check types; leverages existing engine.
- `kind: quality` avoids disclaimer overhead for non-compliance use.

### Negative

- Overlap with eu-ai-act-baseline (EU12-001 ≈ CICD-001; EU12-002 ≈ CICD-002; EU12-003 ≈ CICD-003). Acceptable: starter is adoption wedge; compliance pack is regulatory mapping. Teams using both get deduplicated findings.

### Neutral

- Rule severity (error for CICD-001, warning for CICD-002/003, info for CICD-004) may be tuned based on feedback. Minor version bump if changed.
- CICD-004 is optional (info); implementers may ship v1.0.0 with three rules and add CICD-004 in a patch if preferred.

## Acceptance Criteria

- [ ] Pack at `packs/open/cicd-starter/` with pack.yaml, README, LICENSE
- [ ] Added to `BUILTIN_PACKS`; `assay evidence lint --pack cicd-starter bundle.tar.gz` works
- [ ] **Default pack**: When no `--pack` specified, cicd-starter is used; header shows "Packs: cicd-starter@1.0.0 (default)" + hint
- [ ] CICD-002 uses exact `assay.profile.started` / `assay.profile.finished`; CICD-003 uses canonical paths (traceparent, tracestate, run_id) + data fallback
- [ ] Each rule help_markdown includes "How to fix" bullets + verify command
- [ ] Pack README: Rules table (with Evidence required); pinned GH Action snippet; `--fail-on warning` note; Next steps (soc2, eu-ai-act); provenance framing; CICD-002 customization
- [ ] `packs/open/cicd-starter/` vendored to `crates/assay-evidence/packs/cicd-starter.yaml`
- [ ] ROADMAP §F status table updated on merge

## Appendix A: README template (normative)

The pack README at `packs/open/cicd-starter/README.md` SHALL follow this structure:

```markdown
# CICD Starter Pack

**License:** Apache-2.0 | **Version:** 1.0.0
Minimal traceability for CI pipelines — compatibility floor for adoption.

## Rules

| Rule ID | Check | Severity | Evidence required |
|---------|-------|----------|-------------------|
| CICD-001 | Event presence | error | ≥1 event |
| CICD-002 | Lifecycle pair | warning | assay.profile.started + .finished |
| CICD-003 | Correlation ID | warning | traceparent, tracestate, or run_id |
| CICD-004 | Build identity | info | data.build_id or data.version |

## Quickstart (copy/paste)

    - uses: Rul1an/assay/assay-action@v2   # Pin to commit SHA in production
      with:
        pack: cicd-starter
        fail_on: error   # Use 'warning' to enforce CICD-002/003 in CI

(Bundles auto-discovered; set `bundles` to override glob.)

**Note:** Warnings won't fail CI by default. Set `fail_on: warning` to enforce.

## Next steps

- `--pack soc2-baseline` — SOC2 Common Criteria checks
- `--pack eu-ai-act-baseline` — EU AI Act Article 12 mapping

## Provenance

These checks help build toward provenance maturity (SLSA, attestations).
```

(Full README SHALL also include CICD-002 customization for custom lifecycle types.)

## Open-core / monetization bridge

With cicd-starter as the adoption wedge, the graduation path is:

- **OSS:** cicd-starter + eu-ai-act-baseline + soc2-baseline (marketing-grade entry)
- **Pro/Enterprise:** Pack registry (private packs, signed); policy gates; evidence retention; SARIF + GH Advanced Security; attestation/SLSA packs as upsell after CICD-004

## References

- [ADR-013: EU AI Act Compliance Pack](./ADR-013-EU-AI-Act-Pack.md)
- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [ADR-021: Local Pack Discovery](./ADR-021-Local-Pack-Discovery.md)
- [ADR-022: SOC2 Baseline Pack](./ADR-022-SOC2-Baseline-Pack.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- [ROADMAP §F: Starter Packs (OSS)](../ROADMAP.md#f-starter-packs-oss-p1)
