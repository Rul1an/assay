# Assay Demo Video — Production Brief

## Overview

| Item | Detail |
|------|--------|
| **Title** | "Zero-Flake CI for AI Agents" |
| **Duration** | 60 seconds |
| **Format** | Screen recording + voiceover |
| **Style** | Developer-focused, fast-paced, terminal-centric |
| **Goal** | Show the speed differential (minutes → milliseconds) |
| **CTA** | `pip install assay` / Visit docs.assay.dev |

---

## Script with Timing

### Act 1: The Hook (0:00 - 0:08)

**Voiceover:**
> "Your AI agent tests are flaky. OpenAI times out, the model hallucinates, and your CI fails randomly. Here's how to fix it in 60 seconds."

**Visual:**
- Black screen → Assay logo fade in (2 sec)
- Quick cuts: Red CI badges, "Build Failed", Slack notification spam

**Music:** Subtle tension build (lo-fi electronic)

---

### Act 2: The Problem (0:08 - 0:18)

**Voiceover:**
> "This PR passed locally. But in CI, GPT-4 timed out. Now your team doesn't trust the pipeline anymore."

**Visual:**
```
GitHub Actions UI:
┌─────────────────────────────────────────┐
│ ❌ Run AI Agent Tests                   │
│    Duration: 3m 42s                     │
│                                         │
│    Error: OpenAI API timeout after 30s  │
│    Retry 1/3... failed                  │
│    Retry 2/3... failed                  │
│    Retry 3/3... failed                  │
│                                         │
│    Exit code: 1                         │
└─────────────────────────────────────────┘
```

**Text overlay:** "Sound familiar?"

---

### Act 3: The Solution (0:18 - 0:35)

**Voiceover:**
> "Assay records your agent's behavior once. Then replays it locally — no API calls, no network, no flakiness."

**Visual:** Terminal recording (use asciinema or Screen Studio)

```bash
# Step 1: Record a successful session
$ assay import --format mcp-inspector session.json --init

Imported 47 tool calls
Created: mcp-eval.yaml (with policies)
Created: traces/session-2025-12-27.jsonl

# Step 2: Run tests (instant, offline)
$ assay run --config mcp-eval.yaml --strict

Assay v0.8.0 — Zero-Flake CI for AI Agents

Suite: mcp-basics
Trace: traces/session-2025-12-27.jsonl
```

**Timing notes:**
- Show typing at 2x speed
- Pause on output for readability
- Highlight `--init` flag with subtle glow

---

### Act 4: The Speed (0:35 - 0:45)

**Voiceover:**
> "Milliseconds. Not minutes. Zero cost. Zero flakiness. And when something breaks, you see exactly why."

**Visual:** Test results appearing instantly

```
┌───────────────────┬────────┬─────────────────────────┐
│ Test              │ Status │ Details                 │
├───────────────────┼────────┼─────────────────────────┤
│ args_valid        │ ✅ PASS │ 2ms                     │
│ sequence_valid    │ ✅ PASS │ 1ms                     │
│ tool_blocklist    │ ❌ FAIL │ admin_delete called!    │
└───────────────────┴────────┴─────────────────────────┘

Total: 3ms | 2 passed, 1 failed
Exit code: 1
```

**Text overlay:** 
- "3ms total" (large, green)
- Side comparison: "Traditional: 3+ minutes, $0.50/run"

---

### Act 5: CI Integration (0:45 - 0:52)

**Voiceover:**
> "One line in your GitHub Actions. Every PR gets checked instantly."

**Visual:** GitHub workflow file

```yaml
# .github/workflows/agent-tests.yml
name: Agent Quality Gate

on: [push, pull_request]

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay-action@v1
        with:
          config: mcp-eval.yaml
```

**Cut to:** GitHub PR with green checkmark, "All checks passed" in 4 seconds

---

### Act 6: CTA (0:52 - 0:60)

**Voiceover:**
> "Assay. The purity test for AI. Install it now — link in the description."

**Visual:**
```
┌─────────────────────────────────────────┐
│                                         │
│     pip install assay                   │
│     cargo install assay                 │
│                                         │
│     docs.assay.dev                      │
│     github.com/Rul1an/assay             │
│                                         │
└─────────────────────────────────────────┘
```

**Logo:** Assay logo + tagline "The purity test for AI"

**Music:** Resolve to satisfying end note

---

## Visual Style Guide

### Terminal Theme
- **Font:** JetBrains Mono or Fira Code
- **Background:** #1a1a2e (dark blue-black)
- **Text:** #e0e0e0 (soft white)
- **Accent:** #4ade80 (green for success), #f87171 (red for failure)
- **Prompt:** Simple `$` or custom `assay >`

### Screen Recording Settings
- **Resolution:** 1920x1080 (16:9)
- **Terminal size:** 120 columns x 30 rows
- **Zoom:** 150% for readability
- **Cursor:** Visible but not distracting

### Text Overlays
- **Font:** Inter or SF Pro
- **Style:** Clean, minimal, left-aligned
- **Animation:** Fade in (0.3s), hold, fade out

### Transitions
- Hard cuts between sections (no fancy transitions)
- Exception: Logo fade in/out

---

## Audio

### Voiceover
- **Tone:** Confident, slightly urgent, developer-to-developer
- **Pace:** Fast but clear (150 words/min)
- **Style:** No "marketing speak" — technical and direct

### Music
- **Style:** Lo-fi electronic, subtle
- **Volume:** -20dB under voiceover
- **Suggestions:** 
  - Epidemic Sound: "Coding Session" genre
  - Artlist: "Tech" + "Minimal"

### Sound Effects (optional)
- Keyboard clicks during terminal typing (subtle)
- "Ding" on green checkmark

---

## B-Roll / Cutaways (optional)

If budget allows, quick 1-2 second cuts to:
- Developer frustrated at laptop (stock)
- Server room / data center (for "Air-Gapped" pitch)
- Clock spinning (time savings)

---

## Deliverables Checklist

| Asset | Format | Owner |
|-------|--------|-------|
| Script (final) | Markdown | ✅ This doc |
| Terminal recordings | .mov / .gif | To record |
| Voiceover | .wav / .mp3 | To record |
| Music track | .mp3 | To license |
| Final edit | .mp4 (1080p) | To edit |
| Thumbnail | .png (1280x720) | To design |
| YouTube description | Text | To write |

---

## Distribution Plan

| Platform | Format | Notes |
|----------|--------|-------|
| YouTube | Full 60s | SEO: "AI agent testing", "LLM CI/CD" |
| Twitter/X | 60s native | Pin to profile |
| LinkedIn | 60s native | Target: Engineering Managers |
| GitHub README | Embedded or GIF | First thing visitors see |
| docs.assay.dev | Embedded | Landing page hero |
| Hacker News | Link to YouTube | Launch day |

---

## Recording Checklist

Before recording terminal sessions:

- [ ] Clean terminal history
- [ ] Set terminal theme (dark, high contrast)
- [ ] Increase font size (18-20pt)
- [ ] Hide macOS dock / Windows taskbar
- [ ] Disable notifications
- [ ] Prepare demo files (`session.json`, `mcp-eval.yaml`)
- [ ] Test full flow 2-3 times
- [ ] Use asciinema or Screen Studio for smooth playback

---

## Timeline

| Day | Task |
|-----|------|
| Day 1 | Record terminal sessions |
| Day 2 | Record voiceover |
| Day 3 | First edit |
| Day 4 | Feedback + revisions |
| Day 5 | Final export + distribution |

---

*"Seeing the speed is believing. Show 3ms, don't say 3ms."*
