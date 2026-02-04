# GitHub Projects: gebruik voor open-core (Assay)

**Doel:** Hoe we GitHub Projects inzetten voor het **open-core** deel van Assay, in lijn met best practices en hoe vergelijkbare projecten het doen (februari 2026). Het **enterprise/closed** deel staat niet op de public GitHub.

---

## 1. Hoe vergelijkbare projecten het doen (feb 2026)

### 1.1 GitHub (github/roadmap)

- **Dedicated public repo** `github/roadmap`: alleen roadmap-communicatie; issues zijn read-only, feedback via GitHub Discussions.
- **Labels per item:** release phase (`preview`, `ga`, `in design`, `exploring`), **feature area** (code, planning, security & compliance, …), **feature** (actions, docs, …), **product SKU** (all, github team, github enterprise, …).
- **Project:** officiële roadmap = [org Project](https://github.com/orgs/github/projects/4247); items per **quarter**; “Exploratory” voor items zonder datum.
- **Shipped:** label `shipped` + issue gesloten met link naar Changelog.
- **README:** uitleg release phases, feature areas, disclaimer (geen commitment op datum).

### 1.2 Docker (docker/roadmap)

- **Dedicated public repo** `docker/roadmap`: “Public Roadmap for All Things Docker”; community mag ideeën indienen.
- **Link naar Project:** [See the roadmap »](https://github.com/orgs/docker/projects/51) in README.
- **Board-kolommen (fase):** Shipped, Almost There, We're Writing the Code, Investigating; **geen datums** (“we want room to reprioritise”).
- **Community:** nieuwe issues krijgen label “Proposed”, wekelijkse review; CONTRIBUTING.md voor hoe te contributen.
- **Security:** geen security-issues in public repo; melden via security@docker.com.

### 1.3 AWS SDK for Rust (awslabs/aws-sdk-rust)

- **Public roadmap** via GitHub Project gekoppeld aan de main repo; timeline/board voor geplande werkzaamheden.

### 1.4 Patroon (open-core / enterprise)

| Wat | Waar (public) | Waar (niet public) |
|-----|----------------|---------------------|
| **Roadmap-items** | Issues in public repo (of dedicated roadmap-repo) met labels (phase, area, epic). | Interne prioritering, GTM, sales-milestones. |
| **Planning-view** | Org/repo **Project** (board of roadmap), gelinkt in README. | Private org Project of Notion/Linear. |
| **SKU / tier** | Optioneel label (bijv. “all” vs “enterprise”) als je alleen open-core op de board wilt tonen. | Welke features in welke commercial tier horen. |
| **Feedback** | Issues of Discussions op public repo. | Intern feedback/CRM. |

**Conclusie:** Public = transparantie (wat bouwen we, welke fase); geen datums of weinig datums is gebruikelijk; enterprise/GTM blijft off public GitHub.

---

## 2. Strategie: open-core vs closed enterprise (Assay)

| Domein | Waar het leeft | GitHub Projects |
|--------|----------------|-----------------|
| **Open-core** (OSS roadmap, epics, backlog, PR-tracking) | Public repo + **public** Project(s) | ✅ Eén of meer **public** org/repo Projects; alleen issues/PRs uit **public** repos. |
| **Enterprise** (GTM, private roadmap, sales, premium deliverables) | **Niet** op public GitHub | ❌ Geen enterprise-strategie, data of roadmap in de public repo. Wel: **private** org Project(s) in een private org of internal wiki/Notion. |

**Belangrijk:** Alles wat op de public GitHub staat (incl. public Projects) is open-core. Enterprise roadmap, prioritering en interne milestones horen in een **private** omgeving.

### 2.5 CLI autoriseren (voor gh-commando’s)

Voor `gh project`, `gh issue` en `gh api graphql`: eenmalig `echo JE_PAT | gh auth login --with-token` met een [classic PAT](https://github.com/settings/tokens) (scope **project** + **repo**). Daarna werkt elke `gh`-aanroep. Controleren: `gh auth status`.

---

## 3. Huidige setup en hygiene (public repo)

### 3.0 Canoniek Project (live)

**Project:** [Assay Open-Core Roadmap #4](https://github.com/users/Rul1an/projects/4) — public, gekoppeld aan `Rul1an/assay`. Epics en stories staan als issues in de repo en in het Project. Velden: Stage, Priority. Labels: epic, dx, in design, in progress, in review, shipped, observability, security, compliance (zie §3.2).

- **Status sync (2026-01):** Stage en labels zijn bijgewerkt op basis van [DX-IMPLEMENTATION-PLAN](../maintainers/DX-IMPLEMENTATION-PLAN.md) codebase-verificatie: **Done** + label `shipped` voor E1.1, E1.2, E2.1, E2.2, E2.4, E3.1, E3.3, E3.4, E4.1, E4.2; **In progress** voor E1.3, E2.3, E3.2, E3.5; **Todo** voor E4.3; epics E1–E4 op In progress. **Waarom items nog open staan (DoD):** zie [EPIC-STATUS-REPORT.md](EPIC-STATUS-REPORT.md).
- **Volgorde, tijdlijn, afhankelijkheden, Go/No-Go:** Zie [PROJECT-OPEN-CORE-STRUCTURE.md](PROJECT-OPEN-CORE-STRUCTURE.md) — welke open-core items in welk volgorde, Iteration (Q1–Q4/Backlog), blocked-by, en weergave default gate (Go/No-Go) volgens roadmap en conventies feb 2026.

### 3.1 Eén public Project voor open-core

- **Project:** [Assay Open-Core Roadmap #4](https://github.com/users/Rul1an/projects/4).
- **Scope:** Gekoppeld aan de **public** repo `Rul1an/assay`.
- **Visibility:** **Public** — community en contributors kunnen backlog en voortgang zien.
- **Bron:** Alleen issues en pull requests uit de **public** repository (geen link naar private repos).
- **Ontdekbaarheid:** In README van de repo en in [ROADMAP.md](ROADMAP.md)  → [Project #4](https://github.com/users/Rul1an/projects/4).

### 3.2 Labels (zoals GitHub/Docker: phase + area)

Naast bestaande labels (`epic`, `bug`, `enhancement`, `good first issue`):

- **Release phase / status:** bv. `in design`, `exploring`, `in progress`, `in review`, `shipped` — voor board-kolommen of filter.
- **Feature area:** bv. `observability`, `security`, `dx`, `compliance` — sluit aan op ROADMAP/DX-plan.
- **Optioneel:** geen SKU-labels nodig als alles op de board open-core is; wil je “enterprise” expliciet uitsluiten, dan kan een label `open-core` helpen.

### 3.3 Views (GitHub Projects v2, feb 2026)

Meerdere views op hetzelfde project:

| View | Layout | Doel |
|------|--------|------|
| **Backlog** | Table | Alle open items; groeperen op `epic` of priority; sorteren op created/updated. |
| **Board** | Board | Kanban: kolommen op basis van status (Todo, In progress, In review, Done) of phase (zoals Docker). Optioneel: WIP-limiet per kolom. |
| **Roadmap** | Roadmap | Timeline: **iteration**-veld (quarters) of **date**-veld (target ship); zoom month/quarter/year. Zie [Customizing the roadmap layout](https://docs.github.com/en/issues/planning-and-tracking-with-projects/customizing-views-in-your-project/customizing-the-roadmap-layout). |

- **Iteration field:** ideaal voor quarters (Q1 2026, Q2 2026, …); filter met `@current`, `@next`, of `>`, `<=` voor “na Q2”.
- **Geen vaste datums:** zoals Docker kun je ervoor kiezen alleen fase te tonen (Investigating / In progress / Shipped) en geen target dates op de public board.

### 3.4 Velden (single source of truth)

- **Status** (ingebouwd in Projects V2): gebruik de standaard Status-kolommen of map naar **Stage**.
- **Stage** (custom single select): Todo, In progress, In review, Done — sync met PR/issue state waar mogelijk.
- **Priority** (single select): P0, P1, P2, P3.
- **Epic / Milestone:** labels of custom field (E5, E8, “OpenClaw Telemetry”).
- **Target date** (date) of **Iteration** (quarters): **één** veld voor planning; niet dubbel bijhouden. Iteration voeg je in de Project-UI toe (Settings → New field → Iteration) voor de Roadmap-view.
- **Shipped:** bij afronden: Stage Done + issue sluiten; optioneel label `shipped` + comment met link naar release/CHANGELOG.

### 3.5 Automatisme

- **Built-in workflows:** bij “Issue closed” → status Done; optioneel “Item archived” bij Done + ouder dan X dagen.
- **Auto-add:** items automatisch toevoegen wanneer ze voldoen aan een filter (bv. label `epic` of repo = assay).
- **GitHub Actions** (optioneel): bij “PR ready for review” → status “In review”; bij merge → “Done”. Zie [Automating your project](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project).

### 3.6 Communicatie en structuur

- **Sub-issues:** grote epics opsplitsen in sub-issues; in het Project groeperen op parent of label.
- **Dependencies:** “blocked by” / “blocking” tussen issues (GitHub ondersteunt issue dependencies).
- **Project README/description:** doel van het project, wat elke view toont, link naar [ROADMAP.md](ROADMAP.md) en [DX-IMPLEMENTATION-PLAN.md](../maintainers/DX-IMPLEMENTATION-PLAN.md) als narratieve roadmap.
- **Status updates:** periodiek “On track” / “At risk” + start/target date op projectniveau (Projects ondersteunt status updates).

### 3.7 Open-core vs enterprise (waar wat leeft)

- **Open-core:** Roadmap en epics in **Project #4** en als issues in `Rul1an/assay`. Bron: [ROADMAP.md](ROADMAP.md), [DX-IMPLEMENTATION-PLAN.md](../maintainers/DX-IMPLEMENTATION-PLAN.md). Nieuwe items: `gh issue create` + `gh project item-add 4 --owner Rul1an --url <issue-url>`.
- **Enterprise:** Roadmap, epics en stappen voor commercial/GTM **niet** in de public repo. Beheer in een **private** GitHub Project (andere org), Notion, Linear, enz.

---

## 4. Best practices (conventies)

1. **Single source of truth:** Target date of iteration op één plek; status in sync met issue/PR state.
2. **Project = execution layer:** [ROADMAP.md](ROADMAP.md) en [DX-IMPLEMENTATION-PLAN](../maintainers/DX-IMPLEMENTATION-PLAN.md) zijn de **narratieve** roadmap; Projects is waar **issues/PRs** worden gepland en getracked.
3. **Link project aan repo + README:** Project aan de assay-repo koppelen en in README/ROADMAP een zichtbare link naar het Project (“See the roadmap »”).
4. **Labels = fase + area:** Zoals GitHub/Docker: release phase en feature area als labels, zodat gefilterd en gegroepeerd kan worden.
5. **Templates:** Bij org-level: project-template met vaste views en velden voor hergebruik.
6. **Insights:** Project Insights voor doorloop, burndown per iteration (waar beschikbaar).

---

## 5. Wat niet op de public GitHub hoort

- **Enterprise roadmap** en interne prioritering voor commercial features.
- **GTM- of sales-milestones** en private deliverables.
- **Private org Project(s)** voor intern gebruik — in een **private** GitHub-organization of ander intern systeem beheren.
- **Sensitive metadata** (deal names, internal dates) in issue titles, descriptions of Project-velden in de public repo.

Deze informatie beheer je in een **private** omgeving (private org Projects, Notion, Linear, enz.), niet in dit repo of in public Projects.

---

## 6. Referenties

**Voorbeelden (feb 2026):**

- [GitHub public roadmap](https://github.com/github/roadmap) — README, labels (phase/area/SKU), link naar org Project
- [Docker roadmap](https://github.com/docker/roadmap) — README, link naar org Project, board per fase, CONTRIBUTING, geen datums
- [AWS SDK for Rust roadmap](https://github.com/awslabs/aws-sdk-rust/projects/1) — public Project op repo

**GitHub Docs:**

- [Best practices for Projects](https://docs.github.com/en/issues/planning-and-tracking-with-projects/learning-about-projects/best-practices-for-projects)
- [Managing visibility of projects](https://docs.github.com/en/issues/planning-and-tracking-with-projects/managing-your-project/managing-visibility-of-your-projects)
- [Customizing views](https://docs.github.com/en/issues/planning-and-tracking-with-projects/customizing-views-in-your-project) — table, board, roadmap
- [Iteration fields](https://docs.github.com/en/issues/planning-and-tracking-with-projects/understanding-fields/about-iteration-fields)
- [Automating your project](https://docs.github.com/en/issues/planning-and-tracking-with-projects/automating-your-project)

**Assay-docs:**

- [ROADMAP.md](ROADMAP.md) — strategische roadmap (narratief)
- [DX-IMPLEMENTATION-PLAN](../maintainers/DX-IMPLEMENTATION-PLAN.md) — epics en DoD (narratief)
- [open-core.md](open-core.md) — grens open vs commercial
