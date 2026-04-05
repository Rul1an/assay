# Distribution Channels — Stappenplan & Invulteksten

Per kanaal: link, stappen, en exact wat in te vullen. Laatste verificatie: 2026-03-17.

---

## 4. Hacker News Show HN

### Link
- **Submit:** https://news.ycombinator.com/submit
- **Login:** https://news.ycombinator.com/login (account nodig)

### Stappen
1. Log in op Hacker News
2. Ga naar https://news.ycombinator.com/submit
3. Vul **Title** en **URL** in
4. Klik **Submit**
5. **Belangrijk:** Post direct daarna een eerste comment (binnen 5 min)

### Exact invullen

| Veld | Waarde |
|------|--------|
| **Title** | `Show HN: Assay – The firewall for MCP tool calls, with replayable evidence` |
| **URL** | `https://github.com/Rul1an/assay` |

### Eerste comment (copy-paste, pas aan indien nodig)

```
Assay is a policy-as-code proxy that sits between AI agents (Cursor, Claude, etc.) and their MCP tool servers. Every tool call gets an explicit ALLOW or DENY, and every decision produces a tamper-evident evidence bundle you can audit, diff, and replay.

The problem: the average MCP server scores 34/100 on security (https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c). Assay gives you the policy gate and audit trail to fix that. Covers 7 of 10 OWASP MCP Top 10 risks.

Quick start (2 commands):
  cargo install assay-cli
  assay mcp wrap --policy policy.yaml -- npx @modelcontextprotocol/server-filesystem /tmp/demo

Deterministic — same input, same decision, every time. No hosted backend, no API keys. MIT licensed.
```

### Timing
- **Beste tijd:** dinsdag–donderdag, 8–10 uur EST (14–16 uur CET)
- Reageer op comments gedurende 2+ uur

---

## 5. dev.to Article

### Link
- **Nieuw artikel:** https://dev.to/new
- **Login:** https://dev.to/enter (GitHub/Twitter)

### Stappen
1. Log in via GitHub of Twitter
2. Ga naar https://dev.to/new
3. Vul titel en body in
4. Voeg tags toe: `mcp`, `rust`, `security`, `ai`, `claude`, `cursor`
5. Klik **Publish**

### Exact invullen

| Veld | Waarde |
|------|--------|
| **Title** | `How to add a security gate to your MCP server in 5 minutes` |

### Body (outline — uitwerken tot volledig artikel)

```markdown
## The problem
The average MCP server scores [34/100 on security](https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c). OWASP has a [MCP Top 10](https://owasp.org/www-project-mcp-top-10/) — and most servers don't address it.

## The solution
[Assay](https://github.com/Rul1an/assay) is an open-source policy-as-code proxy. Two commands:

```bash
cargo install assay-cli
assay mcp wrap --policy policy.yaml -- your-mcp-server
```

## Walkthrough
1. Install: `cargo install assay-cli`
2. Create policy (or `assay init --from-trace trace.jsonl`)
3. Wrap your server: `assay mcp wrap --policy policy.yaml -- npx @modelcontextprotocol/server-filesystem /tmp/demo`
4. See ALLOW/DENY in real time
5. Export evidence: `assay evidence export --profile profile.yaml --out bundle.tar.gz`

## CI integration
```yaml
- uses: Rul1an/assay-action@v2
```
SARIF results in GitHub Security tab.

## OWASP coverage
Assay covers 7 of 10 OWASP MCP Top 10 risks. [Full mapping](https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md).

## Links
- [GitHub](https://github.com/Rul1an/assay)
- [MCP Quickstart](https://github.com/Rul1an/assay/tree/main/examples/mcp-quickstart)
- [OWASP MCP Top 10 Mapping](https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md)
```

### Tags
`mcp`, `rust`, `security`, `ai`, `claude`, `cursor`, `devops`

---

## 6. Reddit r/LocalLLaMA

### Link
- **Submit:** https://www.reddit.com/r/LocalLLaMA/submit
- **Subreddit:** https://www.reddit.com/r/LocalLLaMA/

### Stappen
1. Log in op Reddit
2. Ga naar https://www.reddit.com/r/LocalLLaMA/submit
3. Kies **Post** (niet Link als je meer tekst wilt)
4. Vul titel en body in
5. Post
6. Reageer op comments 2+ uur

### Exact invullen

| Veld | Waarde |
|------|--------|
| **Title** | `I built an open-source firewall for MCP tool calls (with replayable audit trail)` |

### Body (copy-paste)

```
MCP servers give AI agents access to tools — files, exec, web, etc. But most score [34/100 on security](https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c). I built [Assay](https://github.com/Rul1an/assay) to fix that.

**What it does:** Sits between your agent and MCP servers. Every tool call gets an explicit ALLOW or DENY based on policy. Every decision produces an evidence bundle you can audit, diff, and replay.

**Quick demo:**
```bash
cargo install assay-cli
assay mcp wrap --policy policy.yaml -- npx @modelcontextprotocol/server-filesystem /tmp/demo
```

**Differentiator:** Deterministic, offline-first, no API keys. Covers 7/10 OWASP MCP Top 10 risks. Evidence bundles are tamper-evident and replayable.

MIT licensed. [GitHub](https://github.com/Rul1an/assay)
```

---

## 7. MCP Server Spot

### Link
- **Submit:** https://www.mcpserverspot.com/submit

### Stappen
1. Ga naar https://www.mcpserverspot.com/submit
2. Vul alle velden in (zie tabel)
3. Klik **Submit Server**

### Exact invullen

| Veld | Waarde |
|------|--------|
| **Server Name** | `Assay` |
| **Description** | `MCP policy firewall with replayable evidence. Block unsafe calls, audit every decision, replay anything. Deterministic policy enforcement.` |
| **Category** | `Security` (of Development als Security niet bestaat) |
| **Features** | ☑ Tools |
| **Status** | `Community` |
| **Icon** | Kies **Server** (of upload icoon) |
| **GitHub** | `https://github.com/Rul1an/assay` |

### Optioneel (MCP Capabilities, Installation, Compatibility)
- **Installation:** `cargo install assay-cli` of `assay mcp wrap --policy policy.yaml -- your-server`
- **Compatibility:** Claude Desktop, Cursor, Windsurf, any MCP client

---

## 8. MCP Marketplace

### Link
- **Creators:** https://mcp-marketplace.io/for-creators
- **Docs:** https://mcp-marketplace.io/docs
- **Submit:** Account aanmaken → dashboard → submit listing

### Stappen
1. Ga naar https://mcp-marketplace.io/
2. Maak account aan (waarschijnlijk via "For Creators" of login)
3. Ga naar creator dashboard / submit
4. Vul listing form in
5. Security scan draait automatisch
6. Pricing: **Free**

### Exact invullen (zo veel mogelijk)

| Veld | Waarde |
|------|--------|
| **Server name** | `Assay` |
| **Description** | `The firewall for MCP tool calls. Policy-as-code enforcement with replayable evidence bundles. Block, audit, replay. Covers 7/10 OWASP MCP Top 10.` |
| **Category** | Security / Developer Tools |
| **Pricing** | Free |
| **GitHub** | `https://github.com/Rul1an/assay` (zonder www. — `www.github.com` faalt de security scan) |
| **Use cases** | MCP security, audit trail, policy enforcement, CI gate |

### Tip
Voeg `LAUNCHGUIDE.md` toe aan de repo om het formulier automatisch te laten vullen. Zie https://mcp-marketplace.io/docs

### "Not Valid MCP" — scanner zoekt naar SDK-import
De scan zoekt in de repo naar: `@modelcontextprotocol/sdk` (of `mcp`, `fastmcp`), server instance, tool/resource/prompt, en transport (stdio/HTTP). Assay is Rust-based; er staat nu een compliance-voorbeeld in `examples/mcp-compliance/` dat aan deze eisen voldoet. Na toevoegen: Edit listing → Re-scan.

---

## 9. Official MCP Registry

### Link
- **Quickstart:** https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/quickstart.mdx
- **Registry:** https://registry.modelcontextprotocol.io
- **Schema:** https://static.modelcontextprotocol.io/schemas/2025-12-11/server.schema.json

### Huidige status (2026-04)
Dit is de primaire registry-route voor Assay, maar nog steeds **preview**. Gebruik hem alleen met een **gegenereerde** `release/server.json` uit een echte release asset set. Zie het als een publish-stap na een release, niet als iets dat al live is omdat de repo packaging klaar heeft.

### Stappen

**Optie A: Script (aanbevolen)**

```bash
cd /path/to/assay
chmod +x scripts/publish-mcp-registry.sh
./scripts/publish-mcp-registry.sh
```

**Optie B: Handmatig**

1. Installeer `mcp-publisher`
2. Ga naar assay repo: `cd /path/to/assay`
3. Login via GitHub auth:
   ```bash
   mcp-publisher login github
   ```
4. Publish de gegenereerde metadata:
   ```bash
   mcp-publisher publish release/server.json
   ```

### Exact invullen
Gebruik **`release/server.json`**, niet een handmatig onderhouden rootbestand. Die metadata hoort gegenereerd te worden uit de echte MCPB release asset plus SHA-256, zodat versie, URL en package type blijven kloppen.

### Wat gebeurt er
1. **Validate:** `mcp-publisher validate` controleert de metadata tegen de officiële registry-regels
2. **Login:** GitHub auth bewijst namespace-eigendom voor `io.github.Rul1an/...`
3. **Publish:** `release/server.json` wordt naar de officiële registry gestuurd

### Troubleshooting
- **Validate faalt** — Controleer eerst of `release/server.json` en de onderliggende `.mcpb` uit dezelfde release komen.
- **Login faalt** — Auth opnieuw met `mcp-publisher login github`.
- **Publish faalt op namespace** — Controleer of de GitHub login overeenkomt met de `io.github.*` namespace in `release/server.json`.
- **Preview drift** — De registry is nog preview; herlees de quickstart als de publish-flow veranderd is.

---

## 10. MCPCentral

### Link
- **Submit guide:** https://mcpcentral.io/submit-server
- **Registry:** https://registry.mcpcentral.io (ander registry dan officieel MCP)

### Huidige status (2026-04)
`registry.mcpcentral.io` resolveert soms niet of blijft onbetrouwbaar beschikbaar. Gebruik deze route daarom alleen met een **gegenereerde** `release/server.json` uit een echte release asset set. Ga er niet vanuit dat Assay al in het Official MCP Registry of automatisch in MCPCentral staat totdat dat publiek verifieerbaar is.

### Stappen (wanneer registry weer bereikbaar is)

**Optie A: Script (aanbevolen)**

```bash
cd /path/to/assay
chmod +x scripts/publish-mcpcentral.sh
./scripts/publish-mcpcentral.sh
```

**Optie B: Handmatig**

1. Installeer mcp-publisher: `brew install mcp-publisher`
2. Ga naar assay repo: `cd /path/to/assay`
3. Auth (opent browser voor GitHub OAuth):
   ```bash
   mcp-publisher login github -registry https://registry.mcpcentral.io
   ```
4. Publish:
   ```bash
   mcp-publisher publish release/server.json
   ```

### Exact invullen
Gebruik **`release/server.json`**, niet een handmatig onderhouden rootbestand. Die metadata hoort gegenereerd te worden uit de echte MCPB release asset plus SHA-256, zodat versie, URL en package type blijven kloppen.

### Wat gebeurt er
1. **Login:** Browser opent voor GitHub OAuth. Autoriseer de MCPCentral app.
2. **Publish:** `release/server.json` wordt naar registry.mcpcentral.io gestuurd.
3. **Resultaat:** Listing verschijnt op https://mcpcentral.io/registry

### Troubleshooting
- **"lookup registry.mcpcentral.io: no such host"** — Registry is down. Wacht en probeer later.
- **"You must be logged in"** — Run `mcp-publisher login github -registry https://registry.mcpcentral.io` opnieuw.
- **Validate faalt** — Controleer eerst of `release/server.json` en de onderliggende `.mcpb` uit dezelfde release komen.

---

## 11. Apigene MCP Directory

### Link
- **Directory:** https://apigene.ai/mcp/official
- **Tools:** https://www.apigene.ai/mcp/tools
- **Blog:** https://apigene.ai/blog/mcp-server-directory

### Status
Apigene lijkt "vendor-verified official servers" te cureren. Er is geen publiek submit-formulier gevonden.

### Stappen
1. Ga naar https://apigene.ai/
2. Zoek contact / "Submit" / "Add" / "For vendors"
3. Of: open issue op hun GitHub (indien aanwezig)
4. Of: mail naar contact op de site

### Suggestie
Stuur mail naar hun contact met:
- **Onderwerp:** `Request to add Assay to MCP Server Directory`
- **Body:** Assay is an open-source MCP policy firewall with evidence bundles. GitHub: https://github.com/Rul1an/assay. Covers 7/10 OWASP MCP Top 10. Would like to be listed in the Apigene directory.

---

## 12. MCP Security Alliance

### Link
- **Contribute:** https://mcp-security-alliance.org/contribute/ of https://mcp-security-alliance.org/contribute/index.html
- **GitHub:** (repo voor PRs — check op site)

### Methode
PR naar hun repo. Content toevoegen aan:
- `resources/` — tools, scripts
- `security/` — best practices

### Stappen
1. Fork de MCP Security Alliance repo (link op contribute-pagina)
2. Maak bestand in `resources/` of `security/`
3. Voeg Assay toe als security tool
4. Open PR

### Voorbeeld `resources/assay.md`

```markdown
# Assay — MCP Policy Firewall

## Overview
Assay is an open-source policy-as-code proxy for MCP tool calls. It sits between AI agents and MCP servers, enforcing explicit ALLOW/DENY decisions with a tamper-evident evidence trail.

## Security Coverage
- MCP08 (Lack of Audit and Telemetry): Complete — evidence bundles, decision logs, replay, diff
- MCP02 (Privilege Escalation): restrict_scope enforcement
- MCP05 (Command Injection): Policy deny, argument validation
- MCP07 (Auth): approval_required, mandate system
- Covers 7/10 OWASP MCP Top 10 risks

## Installation
```bash
cargo install assay-cli
```

## Usage
```bash
assay mcp wrap --policy policy.yaml -- your-mcp-server
```

## References
- [GitHub](https://github.com/Rul1an/assay)
- [OWASP MCP Top 10 Mapping](https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md)
```

---

## 15. AgentSeal Scanner Integration

### Link
- **AgentSeal:** https://github.com/AgentSeal/agentseal
- **Scanner:** Mogelijk onderdeel van agentseal

### Status
Integratie-onderzoek: of Assay als security tool kan worden toegevoegd aan de AgentSeal scanner, of als referentie in hun docs.

### Stappen
1. Bekijk https://github.com/AgentSeal/agentseal
2. Check of er een "integration" of "tools" sectie is
3. Open issue: "Add Assay as recommended MCP policy layer"
4. Of: PR als ze een tools-lijst hebben

---

## 18. Product Hunt

### Link
- **Launch:** https://www.producthunt.com/launch
- **Preparing:** https://www.producthunt.com/launch/preparing-for-launch
- **Help:** https://help.producthunt.com/en/articles/479557-how-to-post-a-product

### Stappen
1. Log in (persoonlijk account, geen company account)
2. Klik **Submit** of **Post** (rechtsboven)
3. Vul product-URL in
4. Vul alle velden in
5. Post
6. Reageer op comments

### Exact invullen

| Veld | Waarde |
|------|--------|
| **Product URL** | `https://github.com/Rul1an/assay` |
| **Product name** | `Assay` |
| **Tagline** | `The firewall for MCP tool calls — with replayable evidence` |
| **Topics** | Developer Tools, AI, Security, Open Source |
| **Pricing** | Free |
| **Thumbnail** | 240×240 px, vierkant (bijv. assay logo of repo screenshot) |
| **Gallery** | Min. 2 afbeeldingen, 1270×760 aanbevolen |
| **Status** | In beta (optioneel) |

### Voorbereiding
- **Tijd:** 12:01 PST voor maximale zichtbaarheid
- **Eerste comment:** Waarom gebouwd, probleem, oplossing, quick start
- **Video:** YouTube demo (optioneel)

---

## 19. AI Agents List

### Link
- **Site:** https://aiagentslist.com/
- **MCP Servers:** https://aiagentslist.com/mcp-servers (of soortgelijke sectie)

### Status
Geen duidelijk submit-formulier gevonden. Mogelijk via contact of "Add" op de site.

### Stappen
1. Ga naar https://aiagentslist.com/
2. Zoek "Submit", "Add", "Contact" of "For creators"
3. Vul formulier in of contacteer via hun contactpagina

### Suggestie voor contact
- **Onderwerp:** `Add Assay — MCP policy firewall`
- **Body:** Assay is an open-source MCP tool-call firewall with replayable evidence bundles. GitHub: https://github.com/Rul1an/assay. Covers 7/10 OWASP MCP Top 10. Would like to be listed.

---

## Overzicht: wat nog gedaan moet worden

| # | Kanaal | Geschatte tijd | Link |
|---|--------|----------------|------|
| 4 | Hacker News | 30 min | https://news.ycombinator.com/submit |
| 5 | dev.to | 1 uur | https://dev.to/new |
| 6 | r/LocalLLaMA | 20 min | https://www.reddit.com/r/LocalLLaMA/submit |
| 7 | MCP Server Spot | 5 min | https://www.mcpserverspot.com/submit |
| 8 | MCP Marketplace | 15 min | https://mcp-marketplace.io/for-creators |
| 9 | Official MCP Registry | 10 min | https://registry.modelcontextprotocol.io |
| 10 | MCPCentral | 10 min | https://mcpcentral.io/submit-server |
| 11 | Apigene | 10 min | Contact via apigene.ai |
| 12 | MCP Security Alliance | 30 min | https://mcp-security-alliance.org/contribute/ |
| 15 | AgentSeal scanner | 30 min | https://github.com/AgentSeal/agentseal |
| 18 | Product Hunt | 1 uur | https://www.producthunt.com/launch |
| 19 | AI Agents List | 10 min | https://aiagentslist.com/ |
