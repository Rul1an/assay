# Samenvatting: CI/CD voor AI Agents — Relevantie voor Assay

> **Bron**: Twee onderzoeksdocumenten (feb 2026): landscape scan + validatie
> **Datum**: 2026-02-08
> **Doel**: Wat betekent dit voor Assay's product, positionering en roadmap?

---

## 1) Marktcontext

- Gartner: 40% enterprise-apps bevat AI agents in 2026, was <5% in 2025.
- Markt groeit van $5,4B (2024) naar >$50B (2030).
- >40% agentic AI projecten wordt tegen eind 2027 geannuleerd door kosten, onduidelijke waarde of inadequate risk controls (Gartner).
- "Agent washing" is een reeel fenomeen — marketing zonder echte capability.

**Implicatie voor Assay**: De governance/audit-hoek is niet nice-to-have maar voorwaarde om agent-projecten door de hype-fase te krijgen. Assay's evidence bundles en compliance packs zitten precies op dat punt.

---

## 2) Wat Assay's aanpak valideert

### "Safe Outputs" patroon (GitHub Next, feb 2026)

GitHub's Continuous AI prototype draait agents standaard **read-only**. Write-acties alleen via "Safe Outputs" — een deterministisch contract dat expliciet definieert welke artifacts een agent mag produceren.

**= Assay's policy-as-code model.** Assay definieert allow/deny/constraints op tool level. Dezelfde filosofie, maar Assay formaliseert het als policy + evidence in plaats van als workflow-configuratie.

### "Agents are software, not models" (Agent CI)

Agent CI's kernframing: geen model registries, geen experiment tracking, geen notebook deployments. Gewoon Git, PRs, branches en CI-gates.

**= Assay's framing.** Trace replay, policy versioning in Git, CI gates via `assay ci`. Assay voegt daarbovenop evidence bundles en compliance, wat Agent CI niet heeft.

### Eval-to-guardrail lifecycle (Galileo)

Pre-productie evaluatiescores worden automatisch runtime governance — scores controleren agent-acties en tool-toegang zonder glue-code.

**= Assay's generate→lock→gate flow.** Profile traces → generate policy → gate in CI. Assay's Wilson-lb gating is een formele versie van dit patroon.

### Fleet of small agents (GitHub Next)

Niet een generieke agent maar veel kleine, elk verantwoordelijk voor een check of taak. Dit is het emergent pattern.

**Implicatie**: Meer agents = meer policies nodig = meer Assay usage. Per-agent policy + per-agent evidence is Assay's sweet spot.

### Policy-as-code als best practice (V2Solutions, Skywork)

Meerdere bronnen noemen policy-as-code, least privilege, audittrails en kill switches als enterprise-vereisten voor agent deployment.

**= Assay's hele bestaansreden.** Belangrijk: dit wordt nu breed als best practice erkend, niet als niche compliance-vereiste.

### Debuggability wint van complexiteit (GitHub Next)

"Developers adopteren transparante, auditeerbare, diff-based patronen — geen opaque systemen die zonder zichtbaarheid acteren."

**= Assay's deterministische replay + explain.** De replay-aanpak is bewust diff-baar en auditeerbaar. `assay explain` past hier perfect in.

---

## 3) Wat Assay nog niet dekt (gaps en kansen)

### OpenTelemetry GenAI Semantic Conventions

De OTel GenAI SIG definieert standaard attributen voor: tasks, actions, agents, teams, artifacts, memory. Nog experimenteel, maar Pydantic AI volgt ze al. Langfuse en Phoenix zijn OTel-native.

**Gap**: Assay's trace format is eigen (JSONL, `assay.trace` schema). Er is geen mapping naar OTel GenAI semconvs.

**Kans**: OTel-compatibele trace export als optionele output. Niet het interne formaat vervangen, maar een bridge bieden. Maakt Assay bruikbaar naast Langfuse/LangSmith observability stacks in plaats van ernaast te staan.

**Prioriteit**: Laag op korte termijn (conventies nog experimenteel), maar strategisch relevant. Volgen, nog niet bouwen.

### Agent-as-a-Judge evaluatie

De evolutie van LLM-as-Judge naar Agent-as-a-Judge: een evaluator die zelf tools, memory en multi-step reasoning inzet om een andere agent's volledige trajectory te beoordelen (niet alleen eindresultaat).

**Status in Assay**: SPRT-inspired adaptive judge met bias-detectie bestaat al. Dit is technisch vooruit op de markt.

**Kans**: Positioneer Assay's judge explicieter als "trajectory evaluation" (niet alleen response evaluation). De term "Agent-as-a-Judge" is herkenbaar in de markt.

### Multi-dimensionale evaluatie (Beyond Task Completion)

Akshathala et al. definiëren vier pijlers: LLM, Memory, Tools, Environment. Conventionele task-completion metrics missen gedragsafwijkingen.

**Status in Assay**: Assay dekt Tools (args_valid, sequence_valid, tool_blocklist) en deels LLM (regex_match, json_schema, semantic_similarity). Memory en Environment zijn niet gedekt.

**Implicatie**: Geen directe actie nodig. Assay's scope is bewust tool/policy-validatie, niet volledige agent-evaluatie. Maar de framing "we doen de Tools-pijler goed" is nuttig voor positionering.

### Progressive deployment (branch-based environments)

Agent CI biedt branch-based deployments: elke branch als live agent-omgeving (dev → staging → prod).

**Status in Assay**: Niet van toepassing — Assay is geen deployment platform. Maar Assay's CI gate integreert met branch protection, wat hetzelfde effect heeft op de "mag dit mergen" beslissing.

### A2A Protocol

Agent-to-agent communicatie via Agent Cards over HTTP. Complementair aan MCP.

**Status in Assay**: Assay focust op MCP (agent→tools). A2A is multi-agent orchestratie.

**Implicatie**: Als multi-agent systemen mainstream worden, moet policy-enforcement ook inter-agent communicatie dekken. Niet nu, maar awareness houden.

### AAIF (Linux Foundation governance voor MCP)

MCP, goose en AGENTS.md zijn ondergebracht bij de Agentic AI Foundation (Linux Foundation, dec 2025). Vendor-neutraal.

**Implicatie voor Assay**: MCP-bet is gevalideerd als langetermijn-standaard. Assay's MCP-focus is de juiste keuze. AAIF-governance vermindert risico op protocol-fragmentatie.

---

## 4) Competitief landschap

| Speler | Wat ze doen | Overlap met Assay | Differentiator Assay |
|--------|-------------|-------------------|---------------------|
| **Agent CI** | Git-native evals, PR-gates, branch deployments, OTel monitoring | PR-gates, eval-on-merge | Evidence bundles, compliance packs, deterministic replay, policy-as-code (niet alleen evals) |
| **Dagger** | Agentic CI runtime, constrained environments, self-healing pipelines | Constrained agent execution | Assay is policy/evidence, niet runtime — complementair |
| **Langfuse** | Open-source observability, tracing, prompt management | Trace capture | Assay is validation/governance, niet observability — complementair |
| **LangSmith** | Developer tracing, eval pipelines, quality-gated deployments | CI eval gates | Assay is framework-agnostisch, LangSmith is LangChain-gebonden |
| **Galileo** | Eval-to-guardrail, ChainPoll, hallucinatie-detectie | Guardrail lifecycle | Assay doet deterministic policy, niet probabilistic guardrails |
| **GitHub gh-aw** | Natural-language workflow → Actions, Safe Outputs | Safe Outputs concept | Assay formaliseert policy + evidence; gh-aw is workflow authoring |
| **Zencoder** | Autonome coding agents in CI | CI-integratie | Assay valideert agents, bouwt ze niet |

**Samenvatting**: Assay's unieke positie is de combinatie van:
1. **Deterministic replay** (geen andere tool doet dit)
2. **Evidence bundles** met integriteitsgaranties (geen concurrent biedt dit)
3. **Policy-as-code** met formele enforcement (Agent CI doet evals, niet policy)
4. **Compliance packs** als commerciele laag (uniek in de markt)

De meeste concurrenten zitten op observability of eval-as-a-service. Assay zit op governance + audit.

---

## 5) Strategische aanbevelingen

### Direct relevant (verwerk in roadmap)

1. **Positionering verscherpen**: "Policy-as-Code for AI Agents" is nu een erkende best practice. Assay hoeft dit concept niet meer uit te leggen — het wordt breed aanbevolen. Focus op "wij doen het beter/formeler dan de rest".

2. **"Safe Outputs" taal adopteren**: GitHub's framing is herkenbaar. Assay's allow/deny policy is hetzelfde concept. Gebruik de term in docs/marketing waar het past.

3. **Evidence bundles als differentiator benadrukken**: Geen enkele concurrent biedt tamper-evident, content-addressed audit artifacts. Dit is Assay's moat voor de compliance/enterprise markt.

4. **Fleet-of-agents use case**: Documenteer hoe Assay per-agent policies managed in een multi-agent setup. Dit wordt het dominante deployment pattern.

### Volgen, niet bouwen (awareness)

5. **OTel GenAI semconvs**: Monitor de standaardisatie. Als conventies stabiel worden, overweeg een `--otel-export` flag op trace output. Geen intern formaat veranderen.

6. **A2A protocol**: Monitor. Als inter-agent policy enforcement relevant wordt, is Assay goed gepositioneerd om het te doen (MCP-proxy pattern uitbreiden naar A2A).

7. **Agent-as-a-Judge terminologie**: Assay's SPRT judge past in dit framework. Gebruik de term waar het de positionering helpt.

### Niet doen

8. **Niet concurreren op observability**: Langfuse/LangSmith/Arize doen dit beter en het is een andere markt. Assay is governance, niet monitoring. Integreer waar nodig (OTel), maar bouw geen dashboard.

9. **Niet concurreren op eval-as-a-service**: Agent CI en LangSmith doen evals. Assay doet policy enforcement + evidence. Overlap is er op PR-gates, maar de waardepropositie is anders.

10. **Niet concurreren op agent-bouw**: Dagger/Zencoder bouwen agents. Assay valideert ze. Complementair, niet competitief.

---

## 6) Referentie-architectuur mapping

Het drie-lagen model (V2Solutions) mapt op Assay:

| Laag | V2Solutions | Assay equivalent |
|------|-------------|-----------------|
| **Observatie** | Telemetrie, logs, metrics uit builds/tests/deploys | Trace capture (JSONL), evidence events (CloudEvents), VCR recordings |
| **Redenering** | LLM/rules interpreteren signalen, stellen acties voor | Policy engine (allow/deny/constraints), Wilson-lb gating, SPRT judge |
| **Actie** | Tests herhalen, rollouts pauzeren, rollbacks | Exit codes (0-4), SARIF upload, PR comments, next_step() suggestions |

Assay dekt alle drie de lagen voor de governance use case. Het mist de "actie" laag voor deployment (rollbacks, canaries) — maar dat is bewust buiten scope.

---

## 7) Key papers om te volgen

| Paper | Waarom relevant |
|-------|----------------|
| Akshathala et al., "Beyond Task Completion" (arXiv:2512.12791) | Vier-pijler evaluatiemodel; valideert dat tool-validatie (Assay's focus) een zelfstandige evaluatie-as is |
| Dong et al., "CAB Framework" (arXiv:2512.23844) | Context-adaptive gedragsverwachtingen; relevant voor hoe packs/policies per use case variëren |
| Yu, "Agent-as-a-Judge" (arXiv:2508.02994) | Trajectory evaluation; valideert Assay's SPRT judge aanpak |
| UIUC, "Agentic Benchmark Checklist" | Outcome/task validity; relevant voor hoe Assay's eigen test assertions gevalideerd worden |
| "SWE-Bench Pro" (arXiv:2509.16941) | Laat zien dat agent-capabilities overschat worden; onderstreept waarde van deterministic replay |
