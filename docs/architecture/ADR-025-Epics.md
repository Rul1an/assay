# ADR-025 Epics — Evidence-as-a-Product

Epics breakdown for [ADR-025](./ADR-025-Evidence-as-a-Product.md). Aligned with [ROADMAP §G/H](../ROADMAP.md#g-reliability-surface--soak-p1-adr-025).

**Design principles (review Feb 2026):** Contract-first; producer vs consumer metadata split; versioned registries; canonical rule IDs; infra vs policy exit codes.

---

## Epic Overview

| Epic | Scope | Roadmap | I1/I2/I3 |
|------|-------|---------|----------|
| **E1** | Soak MVP — CLI + soak-report-v1 (variatiebron of hernoemen) | P1 | I1 |
| **E2** | Manifest x-assay (producer/consumer split) | P2 | I1 |
| **E3** | Pack requires_signals registry (versioned) | P2 | I1 |
| **E4** | Closure + Completeness + Explainability | P2 | I2 |
| **E5** | Attestation (DSSE) + OTEL export | P3 | I3 |

---

## E1: Soak MVP (Iteration 1)

**Goal:** Policy soak testing — N runs with pass^k semantics, machine-readable report. **Semantiek:** echte run-to-run variatie, anders geen reliability-story.

### Variatiebron (kies één voor MVP)

**Optie A (aanbevolen):** Soak = N runs met variatiebron — seed, tool-fault profile, latency, schema drift, prompt perturbations (ε). Pass^k krijgt betekenis: echte run-to-run variance.

**Optie B (MVP fallback):** Hernoem + scope eerlijk — bv. `assay sim verify --repeat N` of `lint --repeat`. Positioneer als stability/verification pipeline (flaky infra / budget detection), niet agent reliability.

### Exit Codes

| Code | Betekenis |
|------|-----------|
| 0 | All pass (pass_all) |
| 1 | ≥1 policy fail |
| 2 | Infra error (budget exceeded, subprocess fail, IO) |

### Acceptance Criteria

- [ ] CLI: `assay sim soak --iterations N --pack <pack> --target <bundle> [--report out.json|-]`
- [ ] Report: `schema_version: "soak-report-v1"`, `pass_rate`, `pass_all`, `decision_policy`, `packs[]` (name, version, digest)
- [ ] `violations_by_rule` keys zijn canonical `pack@ver:rule_id` — consistent met lint output
- [ ] `--decision-policy` CLI (pass_on_severity_at_or_above: error|warning|info); niet alleen in report
- [ ] Schema validation test (golden JSON tegen soak-report-v1)
- [ ] Human summary: pass_rate, first_failure_at, top 3 violated rules
- [ ] `--report -` schrijft naar stdout (CI piping)
- [ ] Pack loader = lint (normative)

### Merge Gates I1

- [ ] Soak report schema validation test
- [ ] Canonical rule-id formatting end-to-end (lint == soak)
- [ ] packs[], limits, seed altijd in report

---

## E2: Manifest x-assay Extensions (Iteration 1)

**Goal:** Provenance in `manifest.json`. **Belangrijk:** producer vs consumer metadata splitsen.

### Producer vs Consumer Split

| Laag | Veld | Inhoud |
|------|------|--------|
| **Producer** | `x-assay.bundle_provenance` | toolchain versions, model identity, run_id, etc. |
| **Consumer** | `x-assay.evaluations[]` | per lint/soak run: packs_applied, decision_policy, results_digest, created_at |

**Reden:** Bundle wordt vaak gemaakt vóórdat packs gekozen worden. "packs_applied" hangt af van consumer, niet producer. Consumer-evaluations zijn append-only per run.

### Acceptance Criteria

- [ ] `manifest.json` krijgt `x-assay.bundle_provenance` (producer-side)
- [ ] `manifest.json` krijgt `x-assay.evaluations[]` (consumer-side): `{packs_applied, decision_policy, results_digest, created_at}`
- [ ] Digest format: `sha256:hex` (expliciet algoritme)
- [ ] Precedence: CLI pack selection > manifest evaluation hints > defaults

### Merge Gates I1

- [ ] Duidelijk producer vs consumer (MVP: alleen evaluations[] oké)

---

## E3: Pack requires_signals Registry (Iteration 1)

**Goal:** Canonical signal registry; packs declareren `x-assay.requires_signals`. **Versie:** registry is versioned.

### Registry v0 (neutralere keys)

| Key | Beschrijving |
|-----|--------------|
| `policy_decisions` | Policy rule outcomes |
| `tool_calls` | Tool invocations |
| `tool_outputs_cache` | Cached tool outputs (closure) |
| `model_identity` | Model ID, provider |
| `prompt_lineage` | Prompt provenance |
| `human_approvals` | Human-in-the-loop decisions |

Per signal: evidence location hints (event types / paths) voor E4 `--explain`.

### Acceptance Criteria

- [ ] Pack: `x-assay.requires_signals_version: "v0"`, `x-assay.requires_signals: [...]`
- [ ] Explicit empty: `requires_signals: []` (missing ≠ empty; verbergt intent)
- [ ] Unknown key: fail met actionable error ("did you mean …?")
- [ ] cicd-starter + eu-ai-act-baseline updaten

### E1→E3 link

Soak report bevat `required_signals[]` metadata (uit packs) zodat E4 closure/completeness voorbereid is.

### Merge Gates I1

- [ ] Registry versioned + actionable errors

---

## E4: Closure + Completeness + Explainability (Iteration 2)

**Goal:** Completeness (pack-relative) + Closure score (replay-relative) + `--explain`. **Transparency:** waarom 0.66?

### Scoring Transparency

Closure output bevat altijd:

- `scoring.method` (bv. `weighted_ratio_v1`)
- `scoring.weights` (default weights)
- `by_signal` detail (per signal: status, reason, evidence_paths)

### Explain Hierarchy

| Topic | Effect |
|-------|--------|
| `--explain closure` | Summary |
| `--explain closure.score` | Closure gaps (waarom niet 1.0) |
| `--explain signals.tool_outputs_cache` | Signal-specifiek |
| `--explain <pack>:<rule>` | Pack-relative, missing signals |

### Acceptance Criteria

- [ ] Completeness matrix pack-relative (per pack, niet alleen global)
- [ ] Closure score 0.0–1.0 uit bundle contents alone (deterministic)
- [ ] closure-v1 schema validation test
- [ ] `--explain closure` werkt zonder packs; met packs toont "missing signals" per pack
- [ ] Future: `--report-kind closure` voor apart closure.json

### Merge Gates I2

- [ ] closure-v1 schema validation
- [ ] --explain closure zonder/met packs

---

## E5: Attestation (DSSE) + OTEL (Iteration 3)

**Goal:** DSSE envelope + offline verify; OTEL met strikte mapping_loss.

### DSSE Payload (minimaal contract)

- Digest(s) van: manifest, events, policy results
- packs + versions + digests
- decision_policy
- closure-v1 digest

### Offline Verify Output

- Signature valid
- payload schema_version(s)
- Content digest match

### mapping_loss (minimaal)

- Welke verplichte signals/attributes konden niet gemapt worden
- Welke semconv version gebruikt
- lossy translations count

### Acceptance Criteria

- [ ] DSSE envelope (feature-flagged)
- [ ] Offline verify: signature, schema_version, digest match
- [ ] OTEL export: GenAI SemConv + mapping_loss sectie (minimaal gedefinieerd)

---

## Dependency Order

```
E1 (Soak) ──┬──► E2 (Manifest evaluations)
            ├──► E3 (requires_signals)  [E1 report bevat required_signals[]]
            └──► E2, E3 kunnen parallel
E2, E3 ────────► E4 (Closure)
E4 ────────────► E5 (Attestation + OTEL)
```

---

## Merge Gates Summary

### Iteration 1 (E1+E2+E3)

- Soak report schema validation (golden JSON)
- Canonical rule-id end-to-end (lint == soak)
- packs[], limits, seed in report
- E2: producer vs consumer (evaluations[] in MVP)
- E3: versioned registry + actionable errors

### Iteration 2 (E4)

- closure-v1 schema validation
- --explain closure zonder/met packs

### Iteration 3 (E5)

- DSSE payload contract
- mapping_loss contract
