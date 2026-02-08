# Demo Strategy: Assay Developer-Focused Launch

> **Status**: Draft
> **Date**: 2026-02-08
> **Goal**: Maximaal developer-bereik met reproduceerbare, CI-geautomatiseerde terminal demo's.

---

## 1) Recording Tooling

### Primair: VHS (Charmbracelet)

Scriptbare `.tape` files — volledig reproduceerbaar, versiebeheer in git.

- Output: GIF, MP4, WebM, PNG-frames in één run
- CI: officiële `charmbracelet/vhs-action` — demo's worden automatisch opnieuw gegenereerd bij code-wijzigingen
- Precieze controle over typing snelheid, pauzes, window grootte, thema
- Installatie: `brew install charmbracelet/tap/vhs`

### Aanvullend: asciinema (v3)

Voor docs-site embedding waar interactieve playback en tekst-selecteerbaarheid belangrijk zijn.

- Rust rewrite (v3, sep 2025) — snelle startup, single binary
- Interactieve JS-player met pause/seek/speed control
- Tekst is kopieerbaar uit de player (dev credibility)
- Tiny file sizes (text-based asciicast format)
- Self-hostable player component

### Vergelijking

| Feature | VHS | asciinema v3 | Terminalizer | t-rec |
|---------|-----|-------------|-------------|-------|
| Taal | Go | Rust | Node.js | Rust |
| Opname | Scripted (tape) | Live capture | Live capture | Screenshot |
| Deterministisch | Ja | Nee | Nee | Nee |
| GIF | Ja | Via converter | Ja | Ja |
| MP4/WebM | Ja | Nee | Nee | MP4 |
| SVG | Nee | Via svg-term-cli | Nee | Nee |
| Interactieve player | Nee | Ja (JS) | Ja (web) | Nee |
| Tekst selecteerbaar | Nee | Ja | Nee | Nee |
| CI automation | Officiële GH Action | Handmatig | Handmatig | Handmatig |
| Actief onderhouden | Ja | Ja | Nee | Matig |

**Besluit:** VHS voor alle GIF/video assets (README, social, landing page). Asciinema voor docs-site (interactief, copy-paste).

---

## 2) Demo Inhoud: 5 Scenes

Elke scene is een apart GIF + samen vormen ze een volledige MP4 walkthrough. Structuur volgt het "input → output" patroon dat het best converteert bij devtools (bron: Evil Martians studie, 100 landing pages).

### Scene 1: "Zero to Gate" (Hero GIF — 8s)

**Doel:** Van niks naar werkende CI gate. Dit is de "< 5 min to first PR gate" wedge claim, live bewezen.

```tape
# demo/hero.tape
Output demo/output/hero.gif
Output demo/output/hero.mp4
Set FontSize 16
Set Width 1000
Set Height 600
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Type "mkdir my-agent && cd my-agent"
Enter
Sleep 500ms

Type "assay init --hello-trace --ci github"
Enter
Sleep 3s

Type "assay run --config eval.yaml"
Enter
Sleep 3s
```

**Wat de dev ziet:** Scaffolding met emoji-output (detectie, generatie, klaar) → test pass met exit code 0.

### Scene 2: "Break & Fix" (Probleem → Oplossing — 12s)

**Doel:** Toon wat er gebeurt als een AI agent iets onveiligs doet, en hoe Assay dat vangt. Developers geloven tools die falen, niet tools die altijd groen zijn.

```tape
# demo/break-fix.tape
Output demo/output/break-fix.gif
Set FontSize 16
Set Width 1000
Set Height 600
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

# Run met unsafe trace → FAIL
Type "assay run --config eval.yaml --trace-file traces/unsafe.jsonl"
Enter
Sleep 3s

Sleep 1s

# Run met safe trace → PASS
Type "assay run --config eval.yaml --trace-file traces/safe.jsonl"
Enter
Sleep 3s
```

**Bron:** `examples/demo/` bevat al `unsafe-policy.yaml`, `common-mistake.yaml`, en `safe-policy.yaml`.

### Scene 3: "Evidence Lint" (Compliance — 6s)

**Doel:** Audit/compliance scanning in één commando — differentiator die niemand anders heeft.

```tape
# demo/evidence-lint.tape
Output demo/output/evidence-lint.gif
Set FontSize 16
Set Width 1000
Set Height 600
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Type "assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline"
Enter
Sleep 4s
```

**Output:** Verified badge → findings met article references → SARIF summary.

### Scene 4: "Attack Simulation" (Security — 6s)

**Doel:** Visueel indrukwekkend — ASCII tabel met attack vectors en blocked/bypassed status.

```tape
# demo/sim.tape
Output demo/output/sim.gif
Set FontSize 16
Set Width 1000
Set Height 600
Set Theme "Catppuccin Mocha"
Set TypingSpeed 40ms

Type "assay sim run --suite quick --target bundle.tar.gz --seed 42"
Enter
Sleep 4s
```

**Output:** Tabel met bitflip/truncate/inject → "Blocked/Passed/Bypassed" per vector.

### Scene 5: "Evidence Explorer TUI" (Wow-factor — 8s)

**Doel:** Interactieve TUI — de visuele "money shot" die deelbaar is.

```tape
# demo/explore.tape
Output demo/output/explore.gif
Set FontSize 16
Set Width 1000
Set Height 700
Set Theme "Catppuccin Mocha"

Type "assay evidence explore bundle.tar.gz"
Enter
Sleep 2s

# Navigeer door events
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Type "j"
Sleep 300ms
Enter
Sleep 2s

Type "q"
Sleep 500ms
```

---

## 3) Asset Pipeline

### Directory structuur

```
demo/
  ├── hero.tape
  ├── break-fix.tape
  ├── evidence-lint.tape
  ├── sim.tape
  ├── explore.tape
  ├── full-walkthrough.tape    # Alle scenes achter elkaar → MP4
  ├── fixtures/                # Pre-built bundles/traces voor reproduceerbare output
  │   ├── bundle.tar.gz
  │   ├── traces/
  │   │   ├── safe.jsonl
  │   │   └── unsafe.jsonl
  │   └── eval.yaml
  └── output/                  # Gegenereerde assets (git-ignored of CI-committed)
      ├── hero.gif
      ├── hero.mp4
      ├── break-fix.gif
      ├── evidence-lint.gif
      ├── sim.gif
      ├── explore.gif
      └── full-walkthrough.mp4
```

### CI Automation

```yaml
# .github/workflows/demo.yml
name: Regenerate Demo Assets
on:
  push:
    paths:
      - 'demo/*.tape'
      - 'crates/assay-cli/**'
    branches: [main]
  workflow_dispatch:

jobs:
  vhs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: charmbracelet/vhs-action@v2
        with:
          path: 'demo/hero.tape'
      - uses: charmbracelet/vhs-action@v2
        with:
          path: 'demo/break-fix.tape'
      - uses: charmbracelet/vhs-action@v2
        with:
          path: 'demo/evidence-lint.tape'
      - uses: charmbracelet/vhs-action@v2
        with:
          path: 'demo/sim.tape'
      - uses: charmbracelet/vhs-action@v2
        with:
          path: 'demo/explore.tape'
      - uses: stefanzweifel/git-auto-commit-action@v5
        with:
          commit_message: "chore: regenerate demo assets"
          file_pattern: 'demo/output/*'
```

Demo GIF's worden automatisch opnieuw gegenereerd als CLI output verandert. README toont altijd actuele output.

---

## 4) Formaat per Kanaal

| Kanaal | Formaat | Lengte | Link naar | Asset |
|--------|---------|--------|-----------|-------|
| GitHub README | GIF (hero) | 8s loop | — | `hero.gif` |
| Landing page hero | MP4 autoplay muted loop | 15s | — | `hero.mp4` |
| Hacker News | Link naar repo | — | GitHub repo | README met hero GIF |
| Twitter/X | MP4 video | 30-45s | Repo of landing page | `full-walkthrough.mp4` of samengesteld |
| Reddit | GIF of link post | 8s | GitHub repo | `hero.gif` of `sim.gif` |
| dev.to | Embedded GIF's in artikel | 6-8s per stuk | Repo + docs | Alle scenes apart |
| LinkedIn | Native MP4 | 30-90s | Landing page | `full-walkthrough.mp4` |
| Docs site | asciinema player | Interactief | — | `.cast` bestanden |
| Discord | Quick GIF | 4-6s | Repo | `sim.gif` of `hero.gif` |

---

## 5) Hacker News Launch

### Timing

**Dinsdag** — 60% hogere gemiddelde peak score dan andere dagen (bron: Show HN survival study, 605 posts).

### Titel

```
Show HN: Assay – Policy-as-Code for AI agents (deterministic replay, evidence bundles, eBPF enforcement)
```

### Post body

Eerste persoon, technisch, understated:

```
I've been building Assay to solve a problem I kept hitting:
how do you test AI agents deterministically in CI,
and prove to auditors what they actually did?

Core loop:
1. Record agent traces (MCP transcripts, API calls)
2. Generate policies from observed behavior
3. Replay deterministically in CI — same trace + same flags = identical outcome
4. Produce evidence bundles for compliance (EU AI Act, SOC2)
5. Attack simulation to prove your gates actually work

Written in Rust. Runs offline. No vendor lock-in. No signup.

The evidence bundle format uses content-addressed events (JCS canonicalization,
SHA-256, Merkle root) — so you can cryptographically prove what an agent did.

https://github.com/...
```

### Kritieke regels

- **Link naar GitHub repo**, niet landing page — HN overindexeert op OSS
- **Geen superlatieven** ("fastest", "best", "first") — HN prikt er doorheen
- **Technische diepte** in comments, niet marketing-speak
- **Eerste persoon** ("I built", "I've been working on")
- **Reageer op elk comment** de eerste 2-3 uur
- **README moet gepolijst zijn** met werkende hero GIF voor je post

### Aanvullende kanalen (zelfde week)

| Dag | Kanaal | Format |
|-----|--------|--------|
| Di | Hacker News (Show HN) | Repo link |
| Di | Twitter/X thread | GIF + korte thread (3-4 tweets) |
| Wo | r/rust | "I built X in Rust" post met GIF |
| Wo | r/programming + r/netsec | Cross-post (max 2-3 subs) |
| Do | dev.to | Technisch artikel "How I built..." |
| Vr | LinkedIn | Native MP4 video, compliance angle |

---

## 6) Landing Page Structuur

Gebaseerd op Evil Martians studie (100 devtool landing pages, 2025):

```
┌─────────────────────────────────────────────────────┐
│  Hero                                                │
│  "Policy-as-Code for AI Agents"                     │
│  [hero.mp4 autoplay muted loop]                     │
│  CTA: [Get Started]  [Docs]                         │
├─────────────────────────────────────────────────────┤
│  Input → Output                                      │
│  Links: eval.yaml snippet (4 regels)                │
│  Rechts: terminal output (pass/fail met kleuren)    │
├─────────────────────────────────────────────────────┤
│  3 Pillars (iconen + korte tekst)                   │
│  ┌──────────────┬──────────────┬──────────────┐     │
│  │ Deterministic│  Evidence    │  Attack      │     │
│  │ Replay       │  Bundles     │  Simulation  │     │
│  │ < 5 min to   │  Audit-ready │  Prove your  │     │
│  │ first gate   │  compliance  │  gates work  │     │
│  └──────────────┴──────────────┴──────────────┘     │
├─────────────────────────────────────────────────────┤
│  Code Example (kopieerbaar)                          │
│  $ assay init --hello-trace --ci github             │
│  $ assay run --config eval.yaml                     │
│  $ assay evidence lint bundle.tar.gz                │
│  $ assay sim run --suite quick                      │
├─────────────────────────────────────────────────────┤
│  Social proof: GitHub stars + "Used by" logos       │
├─────────────────────────────────────────────────────┤
│  Footer: "Open source. No vendor lock-in.           │
│           Runs offline. Written in Rust."           │
└─────────────────────────────────────────────────────┘
```

### Wat NIET doen

- Geen pricing op de landing page (eerst adoptie)
- Geen "book a demo" CTA (devs haten dat)
- Geen marketing-jargon of buzzwords
- Geen feature matrix met 50 items
- Geen flashy animaties — clean en simpel wint

---

## 7) Bleeding Edge

### VHS + CI Auto-Update (nu implementeerbaar)

`.tape` files in repo → CI regenereert GIF's bij elke release → README toont altijd actuele output. Competitief voordeel: de meeste projecten hebben verouderde demo GIF's.

### asciinema-player in Docs (nu implementeerbaar)

Self-hosted JS player component op docs-site. Per feature-sectie een mini-demo die afspeelt on click. `data-start-at` en `data-speed` attributen voor precieze controle.

### "Try in Codespace" Button (lage effort, hoge conversie)

GitHub Codespace met pre-installed Assay + voorbeelden. Dev klikt → terminal → `assay init --hello-trace && assay run`. Hoogste conversie maar kost Codespace minutes.

### Interactieve Browser Demo (toekomst)

WebContainer-based playground of pre-recorded asciinema cast met interactieve player. Assay is Rust/native, dus echte WASM-versie is complex — maar gesimuleerde output met pre-recorded casts is haalbaar.

### AI-Narrated Video (optioneel)

AI text-to-speech (ElevenLabs, OpenAI TTS) over VHS-gegenereerde MP4. Script de narration naast het tape file voor synchronisatie. Produceert YouTube/social-ready content met minimale effort.

---

## 8) Demo Design Principes

### Lengte

| Context | Optimale lengte |
|---------|----------------|
| Hero GIF (README/landing) | 4-8 seconden, loopend |
| Feature GIF | 6-12 seconden |
| Social video (Twitter/LinkedIn) | 30-60 seconden |
| YouTube walkthrough | 2-3 minuten max |

De gemiddelde paginabezoek-duur is < 60 seconden. De hero demo moet waarde communiceren in de eerste 2-3 seconden.

### Wat haakt in de eerste 5 seconden

1. **Snelheid/performance contrast** — laat de tool snel runnen met zichtbare timing
2. **Input → output** — commando tonen, direct resultaat
3. **One-liner magic** — één commando dat iets indrukwekkends doet
4. **Zichtbare, betekenisvolle output** — kleuren, structuur, professionele formatting

### Tone

- Geen marketing-speak
- Technische diepte boven glans
- Laat het product spreken, niet de copy
- Understated > overclaimed

---

## 9) Uitvoeringsplan

| Stap | Actie | Afhankelijkheid |
|------|-------|-----------------|
| 1 | Maak `demo/` directory + `demo/fixtures/` met test data | — |
| 2 | Schrijf `hero.tape` — test lokaal met `vhs demo/hero.tape` | VHS geinstalleerd |
| 3 | Schrijf overige 4 tape files | Fixtures klaar |
| 4 | Genereer alle GIF's — embed hero in README | Tape files werken |
| 5 | Zet `vhs-action` CI workflow op | GIF's committed |
| 6 | Maak asciinema recordings voor docs-site | — |
| 7 | Polijst README met hero GIF + install instructions | GIF klaar |
| 8 | Schrijf HN post (eerste persoon, technisch) | README klaar |
| 9 | Prepareer Twitter thread + Reddit posts | Assets klaar |
| 10 | Launch op dinsdag 9:00 EST | Alles klaar |

---

## Bronnen

- [VHS (Charmbracelet)](https://github.com/charmbracelet/vhs) — Terminal recorder
- [VHS GitHub Action](https://github.com/charmbracelet/vhs-action) — CI automation
- [asciinema v3](https://github.com/asciinema/asciinema) — Interactive terminal recording
- [Evil Martians: 100 devtool landing pages](https://evilmartians.com/chronicles/we-studied-100-devtool-landing-pages-here-is-what-actually-works-in-2025) — Landing page patterns
- [Markepear: Dev tool HN launch](https://www.markepear.dev/blog/dev-tool-hacker-news-launch) — HN strategie
- [Markepear: Developer marketing guide](https://www.markepear.dev/blog/developer-marketing-guide) — Dev marketing
- [Markepear: Landing page examples](https://www.markepear.dev/examples/landing-page) — Voorbeelden
- [Show HN Survival Study](https://asof.app/research/show-hn-survival) — 605 posts geanalyseerd
- [GIF duration best practices](https://fastmakergif.com/blog/gif-frame-rate-duration-best-practices) — Optimale lengte
