# GitHub Project #4 — Open-core structuur, volgorde, tijdlijn en Go/No-Go

**Doel:** Welke open-core zaken in [Project #4](https://github.com/users/Rul1an/projects/4) horen, in welke volgorde, met tijdlijn/afhankelijkheden/Go-No-Go volgens roadmap, DX-plan en conventies (februari 2026).

**Bronnen:** [ROADMAP.md](ROADMAP.md), [DX-IMPLEMENTATION-PLAN.md](../DX-IMPLEMENTATION-PLAN.md), [GITHUB-PROJECTS-USAGE.md](GITHUB-PROJECTS-USAGE.md), [EPIC-STATUS-REPORT.md](EPIC-STATUS-REPORT.md).

---

## 1. Scope: wat wél en niet in het Project

### 1.1 Wel in Project #4 (open-core)

| Bron | Items |
|------|--------|
| **DX plan (default gate + P1)** | Epics E1–E4 (Blessed init, PR feedback, Exit codes, Ergonomie) + hun stories |
| **DX plan (P1 SOTA)** | E5 (Observability/privacy), E6 (MCP Auth), E7 (Judge), E8 (OTel GenAI), E9 (Replay bundle) — alleen OSS-relevante stories |
| **Roadmap Q2 2026** | Collector templates, baseline pack "No Prompt Leakage", Telemetry Surface Guardrails |
| **Roadmap Q3 2026** | OpenClaw supply chain (openclaw-baseline pack, `assay evidence lint --pack openclaw-baseline`) |
| **Roadmap backlog** | Protocol adapters (ACP/UCP/A2A) — adapter trait, OSS adapters; Replay bundle (E9) |

### 1.2 Niet in Project #4 (enterprise / niet-public)

- Managed Evidence Store, Dashboard, Sigstore Keyless (Enterprise)
- EU AI Act Pro Pack, SOC2 Pro, managed workflows
- GTM/sales-milestones, interne datums

---

## 2. Volgorde in het Project (weergavevolgorde)

Volgorde volgt **fase** (P0 → P1 DX → P1 SOTA) en **afhankelijkheden** uit DX-IMPLEMENTATION-PLAN § "Epics: volgorde & afhankelijkheden". Binnen een fase: epics zoals hieronder; stories onder hun epic.

### 2.1 Aanbevolen sorteer-/groepering

| Volgorde | Groep | Items (issues) | Opmerking |
|----------|--------|----------------|-----------|
| 1 | **Default gate (Go/No-Go)** | Zie §4 — geen apart issue; projectbeschrijving + link naar checklist | Gate = "alle P0 criteria groen" |
| 2 | **E1** Blessed init & CI on-ramp | #95 (epic), E1.1/E1.2 (shipped), #101 (E1.3) | P0 eerst, dan P1 |
| 3 | **E2** PR feedback UX | #96 (epic), E2.1/E2.2/E2.4 (shipped), #104 (E2.3) | |
| 4 | **E3** Exit codes & reason code registry | #97 (epic), E3.1/E3.3/E3.4 (shipped), #107 (E3.2), #110 (E3.5) | |
| 5 | **E4** Ergonomie & debuggability | #98 (epic), E4.1/E4.2 (shipped), #113 (E4.3) | |
| 6 | **E5/E8** Observability & privacy | (Epic-issues aanmaken indien gewenst) E5.1/E5.2, E8.1–E8.3 | Roadmap: Step 3 sign-off ✅; Q2: collector templates |
| 7 | **E6** MCP Auth Hardening | Epic + stories (E6a.1–E6a.3, E6b.1, E6.4) | P1 SOTA; blokkeert E7 niet strikt maar wel eerst |
| 8 | **E7** Judge Reliability | (Epic + stories; roadmap: ✅ Complete) | Alleen open sub-items indien nog aanwezig |
| 9 | **E8** OTel GenAI | Zie E5/E8; roadmap: ✅ Step 3 sign-off | |
| 10 | **E9** Replay Bundle | Epic + stories (E9.1–E9.5) | Gebruikt output E7/E8 |
| 11 | **Q2 OpenClaw Telemetry** | Collector templates, baseline pack "No Prompt Leakage" | Losse issues of onder E5/E8 |
| 12 | **Q3 OpenClaw Supply Chain** | openclaw-baseline pack, `assay evidence lint --pack openclaw-baseline` | |
| 13 | **Backlog** | Protocol adapters (ACP/UCP/A2A), E9, overige P2/P3 OSS | Geen vaste datum |

**Project-views:**
- **Board:** groeperen op **Stage** (Todo / In progress / In review / Done).
- **Table (backlog):** groeperen op **Epic** (of label `epic` + title prefix E1/E2/…), sorteren op **Volgorde** (custom number field) of **Iteration**.
- **Roadmap:** groeperen op **Iteration** (Q1–Q4, Backlog).

---

## 3. Tijdlijn (Iteration) — best practices feb 2026

Conventies (GitHub, Docker): quarters of geen datums; "Exploratory"/Backlog voor items zonder target.

### 3.1 Iteration-veld aanbeveling

Voeg in Project #4 een **Iteration**-veld toe (Settings → New field → Iteration):

| Iteration | Gebruik | Voorbeelden |
|------------|---------|-------------|
| **Q1 2026** | Afgerond of in close-out | E1.1, E1.2, E2.1, E2.2, E3.x, E4.1, E4.2, E5/E8 sign-off |
| **Q2 2026** | Huidige focus (open-core) | E1.3, E2.3, E3.2, E3.5, E4.3; collector templates; baseline pack |
| **Q3 2026** | Gepland | E6, OpenClaw supply chain, Telemetry guardrails |
| **Q4 2026** | Optioneel | E9, protocol adapters (eerste versie) |
| **Backlog** | Geen target / later | Verfijningen, P2 OSS, "Defer"-items uit roadmap |

- **Geen vaste datums** in titel/body (zoals Docker); alleen iteration voor richting.
- Filter in Roadmap-view: `@current` = Q2 2026, `@next` = Q3 2026.

### 3.2 Mapping roadmap → Iteration

| Roadmap-sectie | Iteration | Open-core items in Project |
|----------------|-----------|----------------------------|
| Q1 2026 Trust & Telemetry ✅ | Q1 2026 | Alles wat shipped; E5/E8 sign-off |
| Q2 2026 Supply Chain & OpenClaw Telemetry | Q2 2026 | E1.3, E2.3, E3.2, E3.5, E4.3; collector templates; baseline pack |
| Q2/Q3 Telemetry Surface Guardrails | Q2 2026 / Q3 2026 | Zelfde als hierboven |
| Q3 2026 Enterprise Scale (alleen OSS-delen) | Q3 2026 | OpenClaw supply chain pack, E6 |
| Q4 2026 / Backlog | Q4 2026 / Backlog | E9, protocol adapters, overige OSS |

---

## 4. Afhankelijkheden (blocked by / blocking)

Bron: DX-IMPLEMENTATION-PLAN § "Epics: volgorde & afhankelijkheden".

### 4.1 Fase-afhankelijkheden

| Reeks | Relatie | Toelichting |
|-------|---------|-------------|
| **P0 default gate** | E1 (E1.1, E1.2), E2 (E2.1, E2.2), E3 | Parallel waar mogelijk; samen = "default gate ready" |
| **P1 DX** | E1.3, E2.3, E2.4, E4, E5 | E4.1, E4.2, E5 parallel; E4.3 kan na E4.1/E4.2 |
| **P1 SOTA** | E6 → E7 → E8 → E9 | E6 eerst (security); E9 gebruikt output E7/E8 |

### 4.2 Gebruik in GitHub (feb 2026)

- **Issue dependencies (GA aug 2025):** in issues "Blocked by" / "Blocking" invullen (sidebar).
- **Project:** geen apart dependency-veld nodig; volgorde + iteration + deze tabel zijn de bron.
- **Aanbevolen:** voor E6/E7/E8/E9 in de epic-body expliciet vermelden: "Blocked by: E6" etc., en waar mogelijk de GitHub-relation "Blocked by #X" op de story-issues zetten.

### 4.3 Overzicht (voor copy-paste in issues)

| Issue | Blocked by |
|-------|------------|
| Default gate (Go/No-Go) | E1.1, E1.2, E2.1, E2.2, E3.1–E3.4 (alle Done) |
| E1.3 #101 | — |
| E2.3 #104 | — |
| E3.2 #107, E3.5 #110 | — |
| E4.3 #113 | E4.1, E4.2 (Done) |
| E6 (epic) | — |
| E7 (epic) | E6 (aanbevolen) |
| E8 (epic) | E6 (aanbevolen) |
| E9 (epic) | E7, E8 |
| Collector templates / baseline pack | E5/E8 sign-off (Done) |

---

## 5. Go/No-Go weergave (default gate)

### 5.1 Definitie

**Default gate ready** = alle criteria uit [DX-IMPLEMENTATION-PLAN § Default Gate Go/No-Go Checklist](https://github.com/Rul1an/assay/blob/main/docs/DX-IMPLEMENTATION-PLAN.md#default-gate-gono-go-checklist-p0) zijn ✅ (zie dat document voor de tabel).

### 5.2 Weergave in Project #4 (best practices)

1. **Project description / README**
   Korte zin: "Default gate = Go wanneer alle P0-checklistitems in DX-IMPLEMENTATION-PLAN groen zijn." + link naar DX-IMPLEMENTATION-PLAN § Go/No-Go.

2. **Geen apart "Gate"-issue verplicht**
   Optioneel: een issue "Default gate (Go/No-Go)" die je **sluit** zodra de checklist volledig groen is (met comment: "Checklist DX-IMPLEMENTATION-PLAN § Go/No-Go all ✅"). Dan kun je in een Board-view filteren op "Done" voor die issue.

3. **Optioneel custom veld "Gate"**
   Single select: `Go` | `No-Go` | `N/A`. Alleen op het project zelf (niet op elk issue): één "project status" of een fictief item "Default gate" met Gate = Go/No-Go. GitHub Projects heeft geen project-level velden; daarom is de link in de description + optioneel één gesloten issue de eenvoudigste oplossing.

**Aanbevolen:** description van Project #4 bijwerken met:
- Link naar [DX-IMPLEMENTATION-PLAN § Go/No-Go](https://github.com/Rul1an/assay/blob/main/docs/DX-IMPLEMENTATION-PLAN.md#default-gate-gono-go-checklist-p0).
- Zin: "Default gate = **Go** when all P0 checklist items above are ✅."

---

## 6. Samenvatting acties voor Project #4

| Actie | Waar |
|-------|------|
| **Volgorde** | Table-view: groeperen op Epic/label; sorteren op volgorde (E1 → E2 → E3 → E4 → E5/E8 → E6 → E7 → E8 → E9 → Q2 → Q3 → Backlog). |
| **Iteration** | Iteration-veld toevoegen; bestaande issues mappen naar Q1 2026 (Done), Q2 2026 (open), Q3 2026, Q4 2026, Backlog. |
| **Dependencies** | In story-issues "Blocked by" invullen waar van toepassing (E4.3, E6/E7/E8/E9); in epic-bodies dependency-reeks vermelden. |
| **Go/No-Go** | Project description updaten met link naar DX-IMPLEMENTATION-PLAN § Go/No-Go + zin "Default gate = Go when checklist all ✅." |
| **Alleen open-core** | Geen enterprise-issues toevoegen; roadmap-items "Defer"/"Enterprise" niet als open items op het board. |

---

## 7. Referenties

- [GitHub Projects — Customizing the roadmap](https://docs.github.com/en/issues/planning-and-tracking-with-projects/customizing-views-in-your-project/customizing-the-roadmap-layout)
- [GitHub — Iteration fields](https://docs.github.com/en/issues/planning-and-tracking-with-projects/understanding-fields/about-iteration-fields)
- [GitHub — Dependencies on issues](https://docs.github.com/en/issues/managing-your-work-with-issues-and-project-boards/managing-your-work-with-issues-and-project-boards) (blocked by / blocking)
- [ROADMAP.md](ROADMAP.md) — Q1–Q4 + open-core vs enterprise
- [DX-IMPLEMENTATION-PLAN.md](../DX-IMPLEMENTATION-PLAN.md) — Epics volgorde & afhankelijkheden, Go/No-Go checklist
