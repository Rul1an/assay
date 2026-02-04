# Epic-statusrapport — Waarom staan items nog op Todo / In progress?

**Datum:** 2026-01
**Bron:** [DX-IMPLEMENTATION-PLAN](../maintainers/DX-IMPLEMENTATION-PLAN.md), codebase-verificatie, GitHub Project #4.

Dit rapport legt per open item uit wat de **Definition of Done (DoD)** is en **waarom** het nog niet op Done staat.

---

## Definition of Done (DoD) — algemeen

Een story staat op **Done** als:

1. **Acceptance criteria** uit het DX-IMPLEMENTATION-PLAN zijn voldaan (alle [x] voor die story).
2. **Codebase:** relevante code/tests/docs zijn geïmplementeerd en (waar van toepassing) gecontracteerd getest.
3. **Project:** Status = Done, label `shipped` op het issue.

Items die nog **Todo** of **In progress** zijn, voldoen nog niet aan de DoD hieronder.

---

## 1. Items op **Todo**

### E4.3 — Progress UX: N/M tests, ETA (#113)

| | |
|---|--|
| **DoD / acceptance criteria** | Suite met 10+ tests → console toont **progress tijdens de run** (bijv. `3/10`); optioneel ETA. |
| **Waarom nog Todo** | In de code wordt alleen aan het begin "Running N tests..." getoond en aan het eind de summary (passed/failed/…). Er is **geen incrementele progress** (bijv. "Test 3/10 …") tijdens het doorlopen van de tests. |
| **Ontbrekend** | In `assay-core/src/report/console.rs` (of de run-loop in assay-cli) ontbreekt: per afgeronde test een regel of update zoals "3/10 …" of een progress-indicator. Optioneel: ETA op basis van gemiddelde duur. |
| **Ref** | DX-IMPLEMENTATION-PLAN §4.3, Epic E4 acceptance criteria. |

---

## 2. Items op **In progress** (nog niet Done)

### E1.3 — One-click DX demo repos (#101)

| | |
|---|--|
| **DoD / acceptance criteria** | (P1) **CI of smoke:** `assay run` in `examples/dx-demo-node` en `examples/dx-demo-python` slaagt. Concreet: twee example-directories met minimale app, workflow, baseline, README; 0→CI in één flow. |
| **Waarom nog In progress** | De directories **`examples/dx-demo-node/`** en **`examples/dx-demo-python/`** bestaan niet in de repo. Er zijn wel andere examples (bv. `examples/demo/`, `examples/baseline-gate/`), maar geen dedicated "one-click" Node/Python DX-demo met beschreven 0→CI-flow. |
| **Ontbrekend** | Aanmaken van `examples/dx-demo-node/` en `examples/dx-demo-python/` met: minimale app + 1 test, assay config, blessed workflow, traces, README ("0 → CI: clone, run, PR"); plus CI-job of smoke die `assay run` daar uitvoert en exit 0 verwacht. |
| **Ref** | DX-IMPLEMENTATION-PLAN §1.3, Epic E1 acceptance criteria. |

---

### E2.3 — SARIF limits: truncate + N omitted (#104)

| | |
|---|--|
| **DoD / acceptance criteria** | (P1) **Truncatie + "N results omitted"** in run summary en/of SARIF description wanneer het aantal resultaten of de SARIF-grootte de GitHub-limits overschrijdt; configureerbaar. |
| **Waarom nog In progress** | **Evidence lint** heeft dit wel: `--max-results`, truncatie op severity, en "N findings omitted" in JSON/text en in SARIF (`truncated`, `truncatedCount`). De **assay ci / assay run** SARIF (testresultaten uit `assay-core/src/report/sarif.rs`) heeft **geen** truncatie: alle testresultaten gaan naar het bestand; bij veel failures kan upload falen of de limiet overschrijden. |
| **Ontbrekend** | In de pipeline van `assay ci` (assay-core report/sarif): max-aantal-resultaten of max-grootte; bij overschrijding trunceren en in run summary of SARIF-run-description expliciet "N results omitted" (of gelijkwaardig) tonen; optioneel configureerbaar (bijv. `--sarif-max-results` of in config). |
| **Ref** | DX-IMPLEMENTATION-PLAN §2.2 (limits), Epic E2 acceptance criteria. |

---

### E3.2 — summary.json: schema_version, reason_code_version (#107)

| | |
|---|--|
| **DoD / acceptance criteria** | summary.json bevat **`schema_version`**, **`reason_code_version: 1`** en `reason_code` (+ message); versioned en stabiel; golden test waar van toepassing. |
| **Waarom nog In progress** | **`schema_version`** en **`reason_code`** zitten in de code en in de output. Het veld **`reason_code_version`** ontbreekt in de `Summary`-struct in `assay-core/src/report/summary.rs` en wordt dus niet weggeschreven naar summary.json. Downstream tooling kan daardoor niet op een versioned reason-code schema vertrouwen. |
| **Ontbrekend** | In `Summary`: nieuw veld `reason_code_version: u32` (bijv. 1), serialiseren in summary.json; golden test bijwerken zodat `reason_code_version` in de verwachte output zit. |
| **Ref** | DX-IMPLEMENTATION-PLAN §3, Epic E3 acceptance criteria, Go/No-Go checklist #5–6. |

---

### E3.5 — Docs + deprecation run.md, troubleshooting, ADR-019 (#110)

| | |
|---|--|
| **DoD / acceptance criteria** | **run.md** en **troubleshooting.md** in lijn met het feitelijke gedrag; **ADR-019 compatibility** beschreven. Concreet: exit-code-tabel 0/1/2/3 met v2-semantiek (exit 2 = trace not found, exit 3 = infra/judge); reason codes en registry; compat switch `--exit-codes=v1|v2` en `ASSAY_EXIT_CODES`; legacy-notitie (v1: exit 3 = trace not found). |
| **Waarom nog In progress** | **run.md** bevat nog de oude exit-code-tabel: "3 \| Trace file not found". In v2 is trace-not-found exit **2** met reason_code `E_TRACE_NOT_FOUND`; exit 3 = infra/judge. Er ontbreekt documentatie over reason_code-registry, compat switch en `ASSAY_EXIT_CODES`. **troubleshooting.md** beschrijft config errors (exit 2) maar niet expliciet de v2 exit/reason_code-mapping en compat. ADR-019 compatibility (wanneer v1 vs v2, downstream op reason_code schakelen) staat in ADR-019 maar niet als korte samenvatting in run.md. |
| **Ontbrekend** | run.md: exit-code-tabel bijwerken naar v2 (2 = config/trace, 3 = infra/judge); sectie "Reason codes" met link naar registry; sectie "Compat" met `--exit-codes=v1|v2` en `ASSAY_EXIT_CODES`; korte ADR-019-compat-verwijzing. troubleshooting.md: trace not found onder exit 2, judge/infra onder exit 3; verwijzing naar reason_code en run.md. |
| **Ref** | DX-IMPLEMENTATION-PLAN §3, Epic E3 acceptance criteria. |

---

## 3. Epics E1–E4 (Status: In progress)

De **epics** E1–E4 staan op In progress omdat ze **nog open stories** hebben:

| Epic | Open stories | Reden In progress |
|------|----------------|-------------------|
| **E1** | E1.3 | DX demo repos nog niet aanwezig. |
| **E2** | E2.3 | SARIF truncatie voor assay ci nog niet geïmplementeerd. |
| **E3** | E3.2, E3.5 | reason_code_version ontbreekt; run/troubleshooting-docs nog niet v2. |
| **E4** | E4.3 | Progress N/M nog niet geïmplementeerd. |

Zodra bij een epic alle stories Done zijn, kan de epic zelf op Done gezet worden (en eventueel het issue gesloten met label `shipped`).

---

## 4. Samenvatting

| Status    | Aantal items | DoD-kort |
|-----------|--------------|-----------|
| **Todo**  | 1 (E4.3)     | Progress N/M in console tijdens run. |
| **In progress** | 4 stories (E1.3, E2.3, E3.2, E3.5) + 4 epics | DoD per story: zie secties 1–2; epics wachten op hun open stories. |

**Definition of Done** per story = alle acceptance criteria in DX-IMPLEMENTATION-PLAN voor die story voldaan, geverifieerd in code/docs, en in het project Status = Done + label `shipped`.
