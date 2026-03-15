# Adoption Diagnosis Plan (Q2 2026)

## Current State (2026-03-15)

### Signals

| Metric | Value | Read |
|--------|-------|------|
| GitHub Stars | 1 | No social proof |
| Forks | 0 | No community |
| Unique visitors (14d) | 13 | Near-zero organic traffic |
| Binary downloads (all releases) | 20 | Almost nobody downloads |
| crates.io assay-cli | 471 | Likely own CI |
| GitHub Action repos | 2 | Likely only own repo + 1 |
| External contributors | 0 | Solo project |
| PyPI last month | 2,316 | Uninterpretable (mirrors/bots) |

### Diagnosis

The architecture, evidence stack, and experiment quality are strong.
The bottleneck is not security maturity — it is **adoption**.

Primary gaps:
1. **Positioning too broad** — policy firewall + MCP governance + replay system + CI gate + BYOS pipeline + compliance packs + sandbox simultaneously
2. **No 5-minute win** — quickstart requires cargo install + config + trace + multiple commands
3. **No activation funnel** — no data on install → first run → first gate → retention
4. **No social proof** — 0 forks, 0 watchers, disabled discussions
5. **Wedge not chosen** — unclear which of `assay ci`, `mcp wrap`, or `evidence *` is the entry point

## Wedge Decision

### Option analysis

| Wedge | Urgency | Competition | Assay's unique edge |
|-------|---------|-------------|-------------------|
| MCP guardrail (`mcp wrap`) | Growing (MCP adoption rising) | Low (no direct competitor) | Strongest — unique product |
| CI gate (`assay ci`) | Medium | High (promptfoo, langfuse) | Weaker differentiation |
| Evidence/compliance | Later (EU AI Act Aug 2026) | Low but demand also low | Strong but premature |

### Decision: MCP guardrail as primary wedge

**One-line positioning:**

> Stop MCP tool-call regressions before they reach production.

Alternative framings to test:
- "The firewall for MCP tool calls — block, audit, replay."
- "Policy enforcement for MCP servers. One command."

**Why this wedge:**
- No direct open-source competitor does runtime MCP governance with evidence
- MCP adoption is accelerating (AAIF, Claude Desktop, Cursor, Windsurf)
- The experiment trifecta gives credible security credentials
- The `mcp wrap` command already exists and works

## 5-Minute Golden Path

Current path (too many steps):
```
cargo install → assay init → write config → have a trace → assay ci
```

Target path (5 minutes to first value):
```
Step 1: Install (10s)
  cargo install assay-cli
  # or: brew install assay (needs Homebrew formula)

Step 2: Wrap your MCP server (30s)
  assay mcp wrap -- npx @modelcontextprotocol/server-filesystem ./
  # Every tool call shows: ALLOW / DENY + reason

Step 3: See a policy violation (60s)
  # Try a tool call that hits a deny rule
  # Immediate visible feedback: ❌ DENY + reason_code

Step 4: Export evidence (30s)
  assay evidence export --profile profile.yaml --out evidence.tar.gz
  assay evidence verify evidence.tar.gz

Step 5: Add to CI (2min)
  # Copy GitHub Action snippet from README
```

Each step gives immediate, visible feedback. No config file needed for step 2-3.

## Activation Instrumentation

### What to measure

| Funnel Step | Metric | How |
|------------|--------|-----|
| Awareness | README visitors, crates.io page views | GitHub traffic API, crates.io |
| Install | `cargo install assay-cli` count | crates.io downloads (deduplicated) |
| First run | First successful `assay` command | Opt-in telemetry or sample repo CI |
| First gate | First PASS/FAIL outcome | Opt-in telemetry |
| First CI | First GitHub Action run | Action usage search |
| Repeat | Commands per week per user | Opt-in telemetry |
| Retention | Active after 7 days / 30 days | Opt-in telemetry |

### Implementation (minimal, privacy-respecting)

Option A: **Sample repo with instrumented CI** (no user telemetry needed)
- Create `assay-mcp-quickstart` repo with GitHub Action
- Track forks, stars, Action runs as proxy
- Zero privacy concern

Option B: **Opt-in anonymous telemetry**
- `ASSAY_TELEMETRY=1` env var to opt in
- Send: command name, exit code, duration, version. Nothing else.
- Disabled by default
- Display notice on first run

Recommendation: Start with Option A. Add Option B later if needed.

## Adoption Feedback Questions

For 5 conversations with MCP users (Claude Desktop, Cursor, Windsurf users):

1. What is your biggest concern about the MCP tools you use?
2. Have you ever had a tool do something unexpected? What happened?
3. If you could add one safety check to your MCP setup, what would it be?
4. (Show Assay) What do you think this does? (test messaging)
5. (Show `assay mcp wrap` demo) Would you use this? Why / why not?
6. What would stop you from trying this today?
7. What would you need to see before adding this to your CI?

## 30-Day Execution Plan

### Week 1: Wedge + Messaging

- [ ] Rewrite README hero section with MCP guardrail positioning
- [ ] Move "Security Model" and experiment links below the fold
- [ ] Simplify quickstart to 5-minute path (mcp wrap as hero)
- [ ] Remove command table clutter from first screen
- [ ] Add GIF/asciicast of `assay mcp wrap` with visible ALLOW/DENY

### Week 2: Sample Repo + Distribution

- [ ] Create `assay-mcp-quickstart` sample repo
  - 1 MCP server, 1 policy, 1 test, GitHub Action
  - Fork → run → see PASS/FAIL in < 3 minutes
- [ ] Add Homebrew formula for zero-cargo install
- [ ] Test `npx` or `curl | sh` install path for non-Rust users

### Week 3: Social Proof + Discovery

- [ ] Enable GitHub Discussions
- [ ] Write 1 blog post: "How to add a policy gate to your MCP server"
- [ ] Post to: r/LocalLLaMA, Hacker News, MCP Discord/community
- [ ] Add "Used by" or "Try it" section to README (even if just sample repo)

### Week 4: Measure + Iterate

- [ ] Review sample repo forks/stars/Action runs
- [ ] Run 5 user feedback conversations
- [ ] Analyze: where do people drop off?
- [ ] Decide: double down on MCP wrap, or pivot wedge?
- [ ] Update this plan with findings

## What This Means for the Technical Roadmap

### Park (not cancel)

- Boundary erosion experiment (Step 1 frozen, ready when needed)
- Multi-server resolver experiment
- Skill/adapter supply-chain experiment

### Continue

- PR #873 trifecta is merged — use as security credential in positioning
- BYOS store (ADR-015) is complete — useful for evidence export step in golden path
- Structurizr CI — keep as infra hygiene

### New priority

- README/positioning rewrite
- Sample repo with instrumented CI
- Distribution improvements (Homebrew, simpler install)
- User feedback loop

## Success Criteria (30 days)

| Metric | Target | Baseline |
|--------|--------|----------|
| Sample repo forks | > 5 | 0 |
| GitHub stars | > 10 | 1 |
| Unique visitors (14d) | > 50 | 13 |
| User feedback conversations | >= 5 | 0 |
| Identified wedge confidence | High/Medium/Low | Unknown |
