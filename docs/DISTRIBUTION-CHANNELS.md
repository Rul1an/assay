# Distribution Channels — Detailed Execution Plan

Verified March 15, 2026. Each channel tested for actual submission method.

## Priority Order

| # | Channel | Expected impact | Effort | Automatable |
|---|---------|----------------|--------|-------------|
| 1 | mcpservers.org | High (feeds wong2 list, 3,762 stars) | 5 min | No (web form) |
| 2 | TensorBlock awesome-mcp-servers | High (7,260+ repos indexed) | 15 min | Yes (GitHub PR) |
| 3 | AgentSeal awesome-mcp-security | Medium (security-focused) | 5 min | Yes (GitHub issue) |
| 4 | Hacker News Show HN | High if front page | 30 min + engagement | No |
| 5 | dev.to article | Medium (SEO, long-tail) | 1 hour | No |
| 6 | r/LocalLLaMA | Medium (active MCP community) | 20 min + engagement | No |
| 7 | MCP Server Spot | Low-medium | 5 min | No (web form) |
| 8 | MCP Marketplace | Medium | 10 min | No (web form) |
| 9 | MCPCentral | Low-medium | 10 min | Yes (CLI tool) |

---

## 1. mcpservers.org

**URL:** https://mcpservers.org/en/submit
**Method:** Web form (no API)
**Note:** wong2/awesome-mcp-servers (3,762 stars) redirects submissions here.

Fill in:
- **Server Name:** `Assay`
- **Short Description:** `The firewall for MCP tool calls. Block unsafe calls, audit every decision, replay anything. Deterministic policy enforcement with replayable evidence bundles.`
- **Link:** `https://github.com/Rul1an/assay`
- **Category:** `Development`
- **Contact Email:** (your email)

Submit free tier. Approval typically < 24 hours.

---

## 2. TensorBlock/awesome-mcp-servers (7,260+ servers)

**URL:** https://github.com/TensorBlock/awesome-mcp-servers
**Method:** Fork + PR via GitHub API
**Automatable:** Yes

```bash
gh repo fork TensorBlock/awesome-mcp-servers --clone
cd awesome-mcp-servers
# Add to the Security section in README.md:
# - [Rul1an/assay](https://github.com/Rul1an/assay): The firewall for MCP tool calls. Deterministic policy enforcement with replayable evidence bundles. Covers 7/10 OWASP MCP Top 10 risks.
git checkout -b add-assay
# (edit README.md)
git add README.md && git commit -m "Add Assay — MCP policy firewall with evidence trail"
git push -u origin add-assay
gh pr create --repo TensorBlock/awesome-mcp-servers \
  --title "Add Assay — MCP policy firewall with evidence trail" \
  --body "Assay is an open-source MCP policy enforcement proxy with replayable evidence bundles. MIT licensed. Covers 7/10 OWASP MCP Top 10 risks. https://github.com/Rul1an/assay"
```

---

## 3. AgentSeal/awesome-mcp-security (800+ servers)

**URL:** https://github.com/AgentSeal/awesome-mcp-security
**Method:** GitHub issue via API
**Automatable:** Yes

```bash
gh issue create --repo AgentSeal/awesome-mcp-security \
  --title "Add Assay — MCP policy firewall + evidence trail" \
  --body "Assay is an open-source MCP policy enforcement proxy with deterministic evidence bundles. Covers 7/10 OWASP MCP Top 10 risks. Tested with 12 security experiment vectors (0 false positives).

GitHub: https://github.com/Rul1an/assay
OWASP mapping: https://github.com/Rul1an/assay/blob/main/docs/security/OWASP-MCP-TOP10-MAPPING.md"
```

---

## 4. Hacker News

**URL:** https://news.ycombinator.com/submit
**Method:** Manual web submission
**Automatable:** No

- **Title:** `Show HN: Assay – The firewall for MCP tool calls, with replayable evidence`
- **URL:** `https://github.com/Rul1an/assay`
- Post a first comment explaining: problem (34/100 avg score), solution (proxy), differentiator (evidence bundles), quick start (2 commands)
- Best timing: Tuesday-Thursday, 8-10 AM EST

---

## 5. dev.to Article

**URL:** https://dev.to/new
**Method:** Blog post
**Automatable:** No (but content can be drafted)

**Title:** "How to add a security gate to your MCP server in 5 minutes"

Outline:
1. The problem (34/100 stat, OWASP MCP Top 10)
2. The solution (`assay mcp wrap`, 2 commands)
3. Walkthrough (install → wrap → ALLOW/DENY → evidence)
4. CI integration (copy-paste workflow)
5. OWASP coverage (which risks addressed)
6. Links (GitHub, quickstart, OWASP mapping)

---

## 6. Reddit r/LocalLLaMA

**URL:** https://www.reddit.com/r/LocalLLaMA/submit
**Method:** Manual post
**Automatable:** No

**Title:** `I built an open-source firewall for MCP tool calls (with replayable audit trail)`

Body: problem, 2-command demo, differentiator (evidence bundles), GitHub link.
Engage with comments for 2+ hours after posting.

---

## 7. MCP Server Spot

**URL:** https://www.mcpserverspot.com/submit
**Method:** Web form
**Automatable:** No

Fill in:
- **Server Name:** `Assay`
- **Description:** `MCP policy firewall with replayable evidence. Block, audit, replay.` (< 200 chars)
- **Category:** Security
- **Features:** Tools (checked)
- **Status:** Community
- **Icon:** Server

---

## 8. MCP Marketplace

**URL:** https://mcp-marketplace.io/
**Method:** Account + listing form
**Automatable:** No

Create account, submit listing, pass security scan. Set pricing: Free.

---

## 9. MCPCentral

**URL:** https://mcpcentral.io/submit-server
**Method:** CLI tool or web form
**Automatable:** Yes (CLI)

```bash
# Using the server.json already in the repo root
mcp-publisher auth --github
mcp-publisher publish --config server.json
```

---

## What Can Be Automated Right Now

Items 2 and 3 can be executed via `gh` CLI immediately:
- TensorBlock PR (fork + edit + PR)
- AgentSeal issue (create issue)

Items 1, 7, 8, 9 require web form submission (manual, 5-10 min each).
Items 4, 5, 6 require content creation and manual posting.
