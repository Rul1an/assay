# ADR-022: SOC2 Baseline Pack (AICPA Trust Service Criteria)

## Status

Accepted (design). Implementation pending (pack content not yet in repo; ROADMAP "Additional Packs" updated when PR-B merges).

## Context

The open-core roadmap includes a **SOC2 baseline pack** (OSS, Apache-2.0) to map AICPA Trust Service Criteria to evidence-bundle checks, following the same pattern as [ADR-013](./ADR-013-EU-AI-Act-Pack.md) (EU AI Act baseline) and [ADR-016](./ADR-016-Pack-Taxonomy.md) (baseline = direct mapping, no interpretation).

Challenges:
- Official AICPA TSC mappings can be member- or license-restricted; we must avoid copying copyrighted requirement text.
- SOC2 has five TSC categories (Security, Availability, Processing Integrity, Confidentiality, Privacy); scope creep would blur "baseline" vs "Pro."
- Evidence checks (event count, field presence, type existence) verify **presence and integrity** of evidence, not **control effectiveness**; the disclaimer must state this clearly to avoid false confidence.

## Decision

### 1. Scope of v1.0.0: Common Criteria (Security) only

The **soc2-baseline** pack v1.0.0 SHALL map only to **AICPA Trust Services Criteria — Security (Common Criteria)**. Rules MUST NOT claim mapping to **A1 (Availability), PI (Processing Integrity), C (Confidentiality), or P (Privacy)** in v1.0.0. Those categories may be added in a future minor version (e.g. 1.1.0) with explicit mapping tables and disclaimers.

### 2. TSC identifiers and source provenance

- Each rule SHALL reference a **TSC identifier** (e.g. CC6.1, CC7.2) using the **existing** pack schema field **`article_ref`** (per [SPEC-Pack-Engine-v1 Rule Definition](./SPEC-Pack-Engine-v1.md) — same field used for EU AI Act Article refs). Example: `article_ref: "CC6.1"`. No new schema fields; consistent with ADR-013/ADR-016 conventions.
- Rule descriptions SHALL use **short paraphrases** of the control intent, not verbatim copy of AICPA text. Document **source provenance** in the pack README: "TSC identifiers and short paraphrases; see AICPA reference; this pack is not official AICPA guidance."
- Machine-readable TSC references (e.g. [aicpa-soc-tsc-json](https://github.com/CyberRiskGuy/aicpa-soc-tsc-json)) may be cited **in documentation only**. **The pack MUST be fully functional offline;** the pack YAML and runtime behaviour SHALL NOT depend on external URLs or link resolution. External references are documentation-only.

### 3. Check types: no new engine behaviour

Only **existing** Pack Engine check types SHALL be used, with **exact names** from [SPEC-Pack-Engine-v1 Check Types](./SPEC-Pack-Engine-v1.md): `event_count`, `event_pairs`, `event_field_present`, `event_type_exists`, `manifest_field`. No new check types. Rules map TSC to evidence presence and integrity invariants only. *(Expected evidence event types for mapping: lifecycle e.g. `assay.profile.started`/`finished`, decisions e.g. `assay.tool.decision`; mandate events only if a CC control explicitly requires them, otherwise reserve for commerce-baseline.)*

### 4. Disclaimer: evidence presence, not control effectiveness

The pack disclaimer SHALL explicitly state that:

- This pack verifies **evidence presence and integrity invariants** (e.g. that bundles contain events, correlation IDs, lifecycle pairs, or policy-decision events). It does **not** verify **operating effectiveness** of controls or **control design** adequacy.
- **Passing** these checks does not constitute SOC2 compliance and does not imply absence of control issues; **failing** does not by itself constitute an audit failure. Organizations remain responsible for their control environment and audit readiness.

### 5. Pack layout and delivery

- **Location:** `packs/open/soc2-baseline/` with `pack.yaml`, `README.md`, `LICENSE` (Apache-2.0). README SHALL include **licensing hygiene**: "Not affiliated with AICPA. TSC identifiers used for reference only."
- **Built-in:** Optional. Include as built-in (include_str) only if the pack is **widely applicable, stable, and low maintenance**; otherwise distribute via local pack discovery ([ADR-021](./ADR-021-Local-Pack-Discovery.md)) and docs. Prefer **PR-B = content only, PR-C = optional built-in wiring** so that community-first and built-in paths are both supported.
- **Versioning:** Adding new rules is **additive (minor)**. Changing the **meaning** of an existing `rule_id` is **breaking (major)** unless deprecated first. Prevents baseline drift without governance.

### 6. Documentation

- Pack README: mapping table (rule_id ↔ TSC identifier + short paraphrase + check type); "Not affiliated with AICPA"; "Identifiers used for reference."
- SPEC-Pack-Engine-v1 and concepts: examples `--pack eu-ai-act-baseline,soc2-baseline` where relevant.
- ROADMAP "Additional Packs (Future)": mark soc2-baseline as delivered (or in progress) when pack content is merged; reference this ADR.

## Consequences

- SOC2 baseline remains clearly scoped (Common Criteria only in v1.0.0); A1/PI/C/P out of scope for rules in v1.0.0, avoiding ambiguity with Pro/Enterprise interpretations.
- Legal and audit risk is limited by paraphrase + provenance + strong disclaimer; pack is fully functional offline (no external link resolution).
- Baseline stays "direct mapping" only; disclaimer covers presence/integrity vs effectiveness, passing vs absence of issues, and failing vs audit outcome. Rule versioning (additive minor, meaning change = breaking/deprecate) prevents baseline drift.
- **Implementation status:** Until PR-B (pack content) is merged, ROADMAP and docs should reflect "design accepted, implementation pending" to avoid status drift.

## References

- [ADR-013: EU AI Act Compliance Pack](./ADR-013-EU-AI-Act-Pack.md)
- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- AICPA Trust Services Criteria (official reference; use identifiers and paraphrases only)
- [aicpa-soc-tsc-json](https://github.com/CyberRiskGuy/aicpa-soc-tsc-json) (community machine-readable reference)
