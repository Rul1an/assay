# Code Analysis Report: Degradation, Redundancy, Unnecessary Code & AI Comments

> **Date**: 2026-02-09
> **Scope**: Full workspace (`assay-cli`, `assay-core`, `assay-evidence`, `assay-metrics`, `assay-sim`, `assay-registry`)
> **Branch**: `codex/e9c-1-spec-contract-alignment`

---

## Status Refresh (2026-02-10)

This report is the original finding snapshot. For the verified remediation status and
next execution order, see:

- `docs/architecture/RFC-002-code-health-remediation-q1-2026.md`
- `docs/architecture/RFC-004-open-items-convergence-q1-2026.md` (canonical evidence table)

High-level refresh:

- All P1s closed: #1–#7, #9, #10 (merged).
- #8 (`monitor.rs` monolith): addressed via PR #274 (O3/O4/O5 convergence).
- P2/P3 batches from RFC-002 E1-E4 delivered (store consistency, metrics dedup, registry cleanup, comment cleanup).
- `generate.rs` decomposition: **complete** — RFC-003 G1-G6 all merged (PR #271 = G6, `f21c85ef`).
- RFC-002: **complete** (E1-E5 all delivered).
- Remaining structural work tracked in RFC-004 (O6 docs auto-update pending).

---

## Totaal

| Categorie | P1 | P2 | P3 | Totaal |
|-----------|----|----|-----|--------|
| Redundancy | 6 | 10 | 4 | 20 |
| AI Comments | 0 | 2 | 9 | 11 |
| Unnecessary Code | 1 | 4 | 8 | 13 |
| Degradation | 3 | 6 | 5 | 14 |
| **Totaal** | **10** | **22** | **26** | **58** |

---

## P1 — Fix Now

### Redundancy

| # | Locatie | Probleem |
|---|---------|----------|
| 1 | `commands/run.rs:17-31` / `ci.rs:17-31` | Identieke 15-regel error handling match blocks (al in PLAN-pipeline-decomposition) |
| 2 | `storage/store.rs:73-110, 131-170, 199-238` | `TestResultRow` deserialisatie 3x copy-paste met subtiele inconsistentie |
| 3 | `storage/store.rs:520-595` vs `796-868` | `EpisodeGraph` loading (steps + tool_calls SQL) 2x identiek |
| 4 | `lint/engine.rs:214` vs `packs/executor.rs:174` vs `packs/schema.rs:49` | `severity_priority()` 3x gedupliceerd |
| 5 | `lint/mod.rs:11` vs `packs/schema.rs:30` | Twee incompatibele `Severity` enums (`Warn` vs `Warning`) — dwingt `convert_severity()` glue af |
| 6 | `sim/attacks/{integrity,chaos,differential}.rs` | `create_test_bundle()` 3x in hetzelfde crate |

### Unnecessary Code

| # | Locatie | Probleem |
|---|---------|----------|
| 7 | `lint/packs/checks.rs:456-483` | 27-regel `json_pointer_get()` herimplement `serde_json::Value::pointer()` |

### Degradation

| # | Locatie | Probleem |
|---|---------|----------|
| 8 | `commands/monitor.rs:67-663` | 596-regel monolithische `run()` functie |
| 9 | `providers/trace.rs:19-378` | 359-regel `from_path()` parsing functie (8-9 niveaus nesting) |
| 10 | `engine/runner.rs:160-357` | 197-regel `run_test_with_policy()` (5 concerns, 5 niveaus nesting) |

---

## P2 — Fix Soon

### Redundancy

| # | Locatie | Probleem |
|---|---------|----------|
| 11 | `storage/store.rs:752` vs `judge_cache.rs:61` | `now_rfc3339ish()` character-for-character identiek |
| 12 | `llm/openai.rs:100-132` vs `embedder/openai.rs:58-90` | Identiek VCR/HTTP branching pattern |
| 13 | `engine/runner.rs:107-132, 244-256` | Error-case `TestResultRow` constructie 3x gedupliceerd |
| 14 | `lint/sarif.rs:400` + `lint/mod.rs:18` + `packs/schema.rs:40` | `as_sarif_level()` / `severity_to_sarif_level()` 3x |
| 15 | `args_valid.rs:52` + `sequence_valid.rs:69` + `tool_blocklist.rs:24` | Tool call extractie boilerplate 3x identiek |
| 16 | `packs/loader.rs:203` vs `packs/checks.rs:121` | Semver vergelijking 2x met verschillende precisie |
| 17 | `registry/client.rs:396` vs `registry/verify.rs:169` | `compute_digest()` 2x bijna identiek |
| 18 | `evidence/packs/schema.rs:436` vs `registry/reference.rs:221` | Pack name validatie divergeert tussen crates |
| 19 | `args_valid.rs:30-36` vs `sequence_valid.rs:30-36` | Deprecated policy warning block identiek |
| 20 | `commands/fix.rs` vs `doctor.rs` | Identieke `print_unified_diff()` + overlappende concern ownership |

### AI Comments

| # | Locatie | Probleem |
|---|---------|----------|
| 21 | `sim/report.rs:16-38` | `// New:` prefix markers — AI-gegenereerd diff-referentie |
| 22 | `providers/trace.rs:208-211` | `// DEBUG: remove me` commented-out debug block |

### Unnecessary Code

| # | Locatie | Probleem |
|---|---------|----------|
| 23 | `storage/store.rs:258-262` | Dead match arm: `_ => TestStatus::Pass` waar SQL al op 'pass' filtert |
| 24 | `storage/store.rs:297-320` | `insert_run()` vs `create_run()` bijna identiek, inconsistente timestamp formats |
| 25 | `sim/corpus.rs` (29 regels) | Geheel placeholder/stub — elke methode is een no-op |
| 26 | `registry/verify.rs:302-315` | `verify_dsse_signature()` deprecated, `#[allow(dead_code)]`, 0 callers |

### Degradation

| # | Locatie | Probleem |
|---|---------|----------|
| 27 | `commands/generate.rs` (1167 regels) | Types, logic, CLI args, diffing, serialisatie in een bestand |
| 28 | `commands/pipeline.rs` → `runner_builder.rs` | `PipelineInput` destructured terug naar 14 individuele args |
| 29 | `storage/store.rs:299` vs `:308` | Inconsistente timestamp formats (`chrono::to_rfc3339` vs `"unix:N"`) in dezelfde kolom |
| 30 | `storage/store.rs:21-749` + `795-868` | Twee `impl Store` blocks gescheiden door helpers |
| 31 | `metrics/judge.rs:5` EPSILON=1e-9 vs `semantic.rs:6` EPSILON=1e-6 | 3 ordes van grootte verschil, zelfde doel |
| 32 | `metrics/sequence_valid.rs:130-133` | `_ => {}` catch-all slokt onbekende rule variants op met TODO |

---

## P3 — Nice to Have

### Redundancy

| # | Locatie | Probleem |
|---|---------|----------|
| 33 | `helpers.rs` / `util.rs` | `util.rs` is re-export shim voor `helpers.rs` |
| 34 | `commands/init.rs:92-121 / 236-259` | CI scaffolding match 2x in hetzelfde bestand |
| 35 | `lint/packs/checks.rs:442-449` | `convert_severity()` glue functie afgedwongen door dual-enum |
| 36 | `commands/baseline.rs:52-75 / 126-148` | Identieke baseline entry extractie in hetzelfde bestand |

### AI Comments

| # | Locatie | Probleem |
|---|---------|----------|
| 37 | `providers/trace.rs:48-53` | Stream-of-consciousness "Let's use serde_json::Value to sniff" |
| 38 | `engine/runner.rs:389-392` | "Assuming self.incremental is available" — uncertain note over bestaand veld |
| 39 | `report/summary.rs` (10+ locaties) | Doc comments die function names herhalen (`/// Set results summary`) |
| 40 | `errors/mod.rs:237-248` | Genummerde stappen en design notes in code |
| 41 | `errors/diagnostic.rs:73-74` | "we didn't yet" toekomstige-werk note |
| 42 | `commands/config_path.rs` | 5x `=====` section dividers |
| 43 | `commands/monitor.rs:731-747` | 12 regels stream-of-consciousness in test die alleen `assert!(2+2==4)` doet |
| 44 | `commands/runner_builder.rs:193` | "Load baseline if provided" herhaalt code |
| 45 | `lint/packs/checks.rs:12-28` | Doc comments als `/// Pack name.` op veld `pack_name` |

### Unnecessary Code

| # | Locatie | Probleem |
|---|---------|----------|
| 46 | `commands/pipeline_error.rs:13-20` | Overflow check voor 584-miljoen-jaar duratie |
| 47 | `commands/quarantine.rs:22-24` | Ongeimplementeerde stub die success returnt |
| 48 | `commands/discover.rs:103` | Onnodige `.clone()` op servers |
| 49 | `errors/mod.rs:164-166` | `classify_message` triviale wrapper voor `legacy_classify_message` |
| 50 | `errors/diagnostic.rs:73-76` | `format_plain()` identieke pass-through naar `format_terminal()` |
| 51 | `providers/trace.rs:5` + `:363` | Dubbele `use sha2::Digest` import |
| 52 | `registry/verify.rs:197-202` | `compute_digest_raw()` deprecated since 2.11.0, dead |
| 53 | `metrics/judge.rs:87` | `_rationale` wordt geextraheerd en dan weggegooid |

### Degradation

| # | Locatie | Probleem |
|---|---------|----------|
| 54 | `commands/run.rs:33-48` + `ci.rs` | Summary 2x gebouwd, eerste wordt weggegooid |
| 55 | `errors/mod.rs` (6 factory methods) | `detail` gekloond in zowel `message` als `detail` veld |
| 56 | `errors/mod.rs:269` | Typo "incompatbile" → "incompatible" (user-facing) |
| 57 | `lint/sarif.rs:55-60` | Doc comment adverteert deprecated `workingDirectory` |
| 58 | `engine/runner.rs:231-232` | Onnodige `.clone()` op values die direct moved kunnen worden |

---

## Prioritering

### Quick wins (< 1 uur, hoog rendement)

1. `json_pointer_get()` → `serde_json::Value::pointer()` (27 regels weg)
2. `now_rfc3339ish()` naar gedeelde util (identiek in 2 bestanden)
3. `// DEBUG: remove me` + `// New:` markers verwijderen
4. Severity enums unificeren (`Warn` → `Warning` of andersom)
5. Typo "incompatbile" fixen
6. Dead code verwijderen: `corpus.rs`, `verify_dsse_signature`, `compute_digest_raw`

### Al gepland (in PLAN-pipeline-decomposition.md)

- #1 error handling, `elapsed_ms` dedup, reporting extractie

### Architectureel (aparte sprint)

- `monitor.rs` 596-regelfunctie opsplitsen
- `trace.rs` 359-regelfunctie opsplitsen
- `generate.rs` 1167 regels decomponeren
- `store.rs` TestResultRow/EpisodeGraph dedup + timestamp consistentie
- `runner.rs` `run_test_with_policy()` opsplitsen

---

## Relatie met Bestaande Plannen

| Finding | Gedekt door |
|---------|-------------|
| #1 (error handling run/ci) | PLAN-pipeline-decomposition Step 1 |
| #28 (PipelineInput→14 args) | PLAN-pipeline-decomposition (impliciet) |
| #54 (summary 2x gebouwd) | PLAN-pipeline-decomposition Step 4 (optioneel) |
| Overige | Nieuw werk |
