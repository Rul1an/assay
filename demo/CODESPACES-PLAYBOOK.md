# GitHub Codespaces Adoptie-Playbook: Assay

> **Doel**: Van "interessant" naar "ik gebruik dit" in onder 60 seconden
> **Kernmetric**: Time to First Value (TTFV) < 30 seconden na Codespace open
> **Datum**: 2026-02-10

---

## Waarom Codespaces de Ontbrekende Schakel Is

De huidige Assay-installatie vereist:
1. Rust toolchain installeren (als niet aanwezig) — 2-5 minuten
2. `cargo install assay` — 1-3 minuten (compile time)
3. Project opzetten met fixtures — 1-2 minuten
4. Eerste `assay run` — seconden

**Totaal: 4-10 minuten** voordat een developer waarde ziet.

De 15-minuten regel (bron: Daily.dev, 2025) zegt: als een product niet binnen 15 minuten waarde levert, haakt de developer af. Maar de beste tools (Stripe, Supabase, Vercel) halen dit in onder 60 seconden.

Met Codespaces + prebuilds wordt de flow:
1. Klik "Open in Codespaces" — 15-20 seconden (prebuild image laden)
2. Terminal opent, `assay` is al gecompileerd, fixtures staan klaar
3. Typ `assay run` — direct resultaat

**Totaal: onder 30 seconden.** Dat is het verschil tussen "bookmarked for later" en "starred and installed."

---

## De Drie Lagen

### Laag 1: "Open in Codespaces" Badge (Dag 1)

De snelste win. Voeg een badge toe aan je README, boven de installatie-instructies:

```markdown
## Try Assay in 30 Seconds

[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://github.com/codespaces/new?hide_repo_select=true&ref=main&repo=REPO_ID&devcontainer_path=.devcontainer%2Fdevcontainer.json)

No install required. Try our policy engine directly in your browser.
```

**Hoe je je repo ID vindt:**
```bash
gh api repos/OWNER/assay --jq '.id'
```

**Waarom boven installatie-instructies:** Developers scannen README's top-down. De eerste CTA die ze zien bepaalt de conversie. "Try in browser" heeft lagere drempel dan `cargo install`.

**UX flow voor de bezoeker:**

```
Leest README → ziet badge → klikt
    ↓
GitHub authenticatie (als nodig)
    ↓
Codespace provisioning (15-20s met prebuild)
    ↓
VS Code opent in browser, terminal ready
    ↓
Welcome message verschijnt met instructies
    ↓
Typt: make demo
    ↓
Ziet FAIL → PASS output, begrijpt de tool
    ↓
🌟 Stars de repo
```

### Laag 2: Prebuild-Configuratie (Week 1)

Zonder prebuild wacht de gebruiker 3-8 minuten op `cargo build`. Met prebuild laadt het in 15-20 seconden.

**Hoe prebuilds werken:**
1. GitHub draait periodiek een Codespace op de achtergrond
2. Voert `onCreateCommand` uit (compileert Rust, installeert tools)
3. Slaat het complete container image op als cache
4. Wanneer een gebruiker opent, krijgt die het gecachte image — instant klaar

**Kosten:** ±1-2 core hours per week uit je gratis tegoed (120 core hours/maand). Ruim voldoende.

**Setup:**
1. Ga naar repo → Settings → Codespaces → Set up prebuild
2. Selecteer branch: `main`
3. Selecteer regio: Europe West (of waar je publiek zit)
4. Trigger: push to main + schedule (wekelijks)

### Laag 3: Template Repository (Week 2-3)

Een apart `assay-quickstart` repo dat developers forken als startpunt voor hun eigen projecten:

```
assay-quickstart/
├── .devcontainer/
│   └── devcontainer.json
├── policies/
│   ├── basic-safety.yaml        # Simpele allow/deny policy
│   ├── eu-ai-act-baseline.yaml  # Compliance voorbeeld
│   └── custom-example.yaml      # Lege template om mee te starten
├── traces/
│   ├── safe-agent.jsonl         # Agent die binnen beleid blijft
│   └── risky-agent.jsonl        # Agent die buiten beleid gaat
├── eval.yaml                    # Assay eval configuratie
├── Makefile                     # make demo, make test, make lint
└── README.md                    # Quickstart guide
```

**Waarom een apart repo:**
- Hoofdrepo (`assay`) is de tool zelf — complex, veel code
- Template repo is het startpunt voor gebruikers — minimaal, begrijpelijk
- Voorkomt dat beginners verdwalen in de broncode
- "Use this template" knop is prominent op GitHub

---

## Concrete Bestanden

### .devcontainer/devcontainer.json

```json
{
  "name": "Assay Policy Engine",
  "image": "mcr.microsoft.com/devcontainers/rust:1-bookworm",

  "features": {
    "ghcr.io/devcontainers/features/github-cli:1": {}
  },

  "onCreateCommand": "cargo install --path . --locked 2>/dev/null || cargo install assay --locked",

  "postCreateCommand": "bash .devcontainer/welcome.sh",

  "customizations": {
    "vscode": {
      "settings": {
        "terminal.integrated.defaultProfile.linux": "bash",
        "terminal.integrated.fontSize": 14,
        "workbench.colorTheme": "One Dark Pro"
      },
      "extensions": [
        "rust-lang.rust-analyzer",
        "tamasfe.even-better-toml",
        "redhat.vscode-yaml"
      ]
    }
  },

  "remoteUser": "vscode"
}
```

**Ontwerpkeuzes:**
- `onCreateCommand` compileert de binary — dit wordt gecacht door prebuilds
- `postCreateCommand` toont welkomstbericht — draait elke keer (licht, snel)
- Geen `forwardPorts` — Assay is een CLI tool, geen webserver
- `rust-analyzer` + YAML/TOML extensies — de bestanden die je bewerkt
- One Dark Pro theme — visueel consistent met Catppuccin Mocha terminal

### .devcontainer/welcome.sh

```bash
#!/bin/bash
set -e

# Verify assay is available
if ! command -v assay &> /dev/null; then
    echo "⚠️  Assay binary not found. Building from source..."
    cargo install --path . --locked 2>/dev/null || cargo install assay --locked
fi

VERSION=$(assay --version 2>/dev/null || echo "unknown")

cat << 'EOF'

  ╔══════════════════════════════════════════════════════════════╗
  ║                                                              ║
  ║   ┌─┐┌─┐┌─┐┌─┐┬ ┬                                          ║
  ║   ├─┤└─┐└─┐├─┤└┬┘   Policy-as-Code for AI Agents           ║
  ║   ┴ ┴└─┘└─┘┴ ┴ ┴                                           ║
  ║                                                              ║
  ╠══════════════════════════════════════════════════════════════╣
  ║                                                              ║
  ║   Quick start:                                               ║
  ║                                                              ║
  ║     make demo        Run the full break & fix demo           ║
  ║     make test        Test a safe trace against policy        ║
  ║     make fail        Test an unsafe trace (expect failure)   ║
  ║     make explore     Open the TUI evidence explorer          ║
  ║                                                              ║
  ║   Or run directly:                                           ║
  ║                                                              ║
  ║     assay run --config eval.yaml \                           ║
  ║       --trace-file traces/safe.jsonl                         ║
  ║                                                              ║
  ║   Docs:  https://assay.dev/docs                              ║
  ║   Repo:  https://github.com/...                              ║
  ║                                                              ║
  ╚══════════════════════════════════════════════════════════════╝

EOF

echo "  Assay $VERSION ready."
echo ""
```

### Makefile

```makefile
.PHONY: demo test fail explore init clean help

FIXTURES := demo/fixtures
CONFIG   := $(FIXTURES)/eval.yaml
SAFE     := $(FIXTURES)/traces/safe.jsonl
UNSAFE   := $(FIXTURES)/traces/unsafe.jsonl
POLICY   := $(FIXTURES)/policy.yaml

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

demo: fail test ## Run the full break & fix demo
	@echo ""
	@echo "  ✅ Demo complete. The unsafe trace failed, the safe trace passed."
	@echo "  That's Assay: deterministic policy enforcement for AI agents."

test: ## Run a safe trace against policy (should PASS)
	@echo "━━━ Safe trace (expect PASS) ━━━"
	assay run --config $(CONFIG) --trace-file $(SAFE)

fail: ## Run an unsafe trace against policy (should FAIL)
	@echo "━━━ Unsafe trace (expect FAIL) ━━━"
	-assay run --config $(CONFIG) --trace-file $(UNSAFE)

explore: ## Open the TUI evidence explorer
	assay evidence explore $(FIXTURES)/bundle.tar.gz

validate: ## Validate traces against policy
	assay validate --config $(CONFIG) --trace-file $(UNSAFE)
	assay validate --config $(CONFIG) --trace-file $(SAFE)

init: ## Initialize a new Assay project in current directory
	assay init --hello-trace --ci github

clean: ## Remove generated artifacts
	rm -rf .assay/ bundle.tar.gz
```

---

## README Integratie

Voeg dit blok toe aan je README.md, direct onder de hero GIF en projectbeschrijving, **vóór** de installatie-instructies:

```markdown
## Try It Now

[![Open in GitHub Codespaces](https://github.com/codespaces/badge.svg)](https://github.com/codespaces/new?hide_repo_select=true&ref=main&repo=REPO_ID)

No install required. Opens a browser-based terminal with Assay pre-compiled and demo fixtures ready.

```bash
# In the Codespace terminal:
make demo        # See the full break & fix flow
make test        # Run a safe trace (PASS)
make fail        # Run an unsafe trace (FAIL)
```

Or install locally:

```bash
cargo install assay
assay init --hello-trace --ci github
assay run --config eval.yaml
```
```

**Waarom deze volgorde:**
1. Hero GIF (visueel bewijs dat het werkt)
2. "Try It Now" Codespace (laagste drempel)
3. Local install (voor wie overtuigd is)
4. Docs/features (voor wie meer wil weten)

De meeste README's doen het andersom (install eerst, try-it-now later of nooit). Dat is achterhaald — de trend in 2026 is "experience before commitment" (bron: Daytona's 4.000 sterren in week 1 door README-optimalisatie).

---

## CI Workflow: Prebuild Automatisering

```yaml
# .github/workflows/codespaces-prebuild.yml
name: Codespaces Prebuild

on:
  push:
    branches: [main]
    paths:
      - 'Cargo.toml'
      - 'Cargo.lock'
      - 'crates/**'
      - '.devcontainer/**'
  schedule:
    - cron: '0 6 * * 1'  # Maandag 6:00 UTC (voor de werkweek)
  workflow_dispatch:

# GitHub bouwt het prebuild image automatisch op basis van
# .devcontainer/devcontainer.json — je hoeft alleen de trigger te configureren.
# Ga naar: Settings → Codespaces → Set up prebuild
```

**Let op:** De daadwerkelijke prebuild wordt geconfigureerd via de GitHub UI (Settings → Codespaces → Prebuilds), niet via een workflow file. De workflow hierboven triggert alleen een rebuild wanneer relevante bestanden veranderen.

---

## Conferentie & Webinar Strategie

### Pre-Talk Setup

1. Open 2-3 Codespaces vooraf (backup bij trage starts)
2. Zet slides klaar met clickable Codespace link
3. Test of de prebuild actueel is

### Slides Template

```markdown
# Live Demo

Volg mee:

[Open Assay in Codespaces →](https://github.com/codespaces/new?repo=REPO_ID)

Of scan de QR code:

[QR code naar dezelfde URL]
```

### Demo Script (2 minuten)

```
"Open je Codespace. Je ziet een terminal met Assay klaar.
Typ: make fail

[wacht 3 seconden]

Je agent probeerde 'exec' aan te roepen. Dat mag niet volgens je policy.
Exit code 1. In CI blokkeert dit je deploy.

Typ nu: make test

[wacht 3 seconden]

Zelfde tool, zelfde policy, maar de trace is veilig.
Exit code 0. Groene CI. Deterministic — dezelfde trace geeft altijd
hetzelfde resultaat.

Dat is Assay: policy-as-code voor AI agents.
In je terminal, in je CI, offline."
```

### Post-Talk Follow-Up

Stuur de Codespace link in de chat/Slack van het event. Developers die het live hebben gezien klikken 3-5x vaker dan koude traffic (bron: conferentie engagement data).

---

## Beperkingen: Wat Werkt NIET in Codespaces

### Kernel-level Features

Als Assay LSM hooks, eBPF tracepoints, of kernel modules gebruikt voor runtime enforcement, werken deze **niet** in Codespaces containers. Containers hebben geen toegang tot de host kernel.

**Oplossing:** Bied een "userspace mode" aan voor Codespaces:
- Policy validatie: werkt (puur data-analyse)
- Deterministic replay: werkt (trace parsing)
- Evidence bundling: werkt (file I/O)
- Runtime enforcement (LSM/eBPF): werkt NIET

Communiceer dit expliciet:
```
Note: Codespaces runs Assay in validation mode.
For runtime enforcement (LSM/eBPF), install locally on a Linux host.
```

### Performance

- Disk I/O is trager dan lokaal (network-mounted filesystem)
- Eerste `cargo build` zonder prebuild: 3-8 minuten
- Met prebuild: ±20 seconden (opgelost)

### Interactieve TUI

De TUI evidence explorer (`assay evidence explore`) kan beperkingen hebben in de browser-based VS Code terminal:
- Muis-events werken mogelijk niet
- Window resizing kan glitchen
- Kleurweergave kan afwijken

**Test dit voordat je het promoot.** Als de TUI goed werkt in Codespaces, is het een killer demo-feature. Als niet, laat het weg uit de Codespace experience en verwijs naar lokale installatie.

---

## Metrics: Wat Meet Je

| Metric | Hoe | Doel (maand 1) |
|--------|-----|----------------|
| Codespace opens | GitHub repo traffic → referrers | 50+ |
| `make demo` runs | Optioneel: analytics in welcome.sh | 30+ |
| Star velocity | GitHub star history | 2-4x uplift vs. baseline |
| Crates.io downloads | crates.io/crates/assay | Groei na launch |
| README badge clicks | GitHub traffic analytics | CTR > 5% |

**Privacy-first approach:** Voeg GEEN telemetry toe aan de Codespace welcome.sh. Developers haten dat, en het tast je "runs offline, no telemetry" positionering aan. Meet via GitHub's eigen analytics.

---

## Alternatieven voor Wie Geen GitHub Account Heeft

### DevPod (Open Source, Self-Hosted)

DevPod gebruikt dezelfde `devcontainer.json` standaard, maar draait lokaal:

```bash
# Installeer DevPod
brew install devpod

# Open Assay in een lokale container
devpod up https://github.com/OWNER/assay
```

**Voordelen:**
- Geen GitHub account nodig
- Draait lokaal (privacy-first)
- 5-10x goedkoper dan Codespaces
- Zelfde devcontainer.json, geen extra configuratie

**Voeg toe aan README:**
```markdown
Or use [DevPod](https://devpod.sh/) for a local container:
```bash
devpod up https://github.com/OWNER/assay
```
```

### curl | sh Installer (Snelste Lokale Installatie)

Voor developers die lokaal willen installeren zonder Rust toolchain:

```bash
curl -sSf https://assay.dev/install.sh | sh
```

**Vereist:**
- Pre-built binaries voor Linux (x86_64, aarch64) en macOS (x86_64, aarch64)
- GitHub Releases met checksums
- Installatiescript dat platform detecteert en juiste binary downloadt

**Security best practices:**
- HTTPS only, nooit HTTP
- Checksums op een apart account/domein
- Scriptsource in git met PR review
- Wrap alle code in shell functies (voorkomt partial execution bij verbindingsverlies)

---

## Uitvoeringsplan

| Dag | Actie | Effort | Impact |
|-----|-------|--------|--------|
| 1 | Maak `.devcontainer/devcontainer.json` + `welcome.sh` | 30 min | Hoog |
| 1 | Voeg Makefile toe met demo/test/fail targets | 30 min | Hoog |
| 1 | Voeg Codespace badge toe aan README | 5 min | Hoog |
| 2 | Test Codespace end-to-end (open → make demo → werkt?) | 1 uur | Kritiek |
| 2 | Configureer prebuild via GitHub UI | 15 min | Hoog |
| 3 | Test TUI explorer in Codespace terminal | 30 min | Medium |
| 7 | Maak `assay-quickstart` template repo | 2 uur | Medium |
| 7 | Voeg DevPod instructies toe aan README | 15 min | Laag |
| 14 | Pre-built binary installer (curl \| sh) | 4 uur | Hoog |
| 14 | Prepareer conferentie demo-script | 1 uur | Medium |

**Totaal:** ±10 uur voor het complete Codespace adoptie-pad. De eerste dag levert 80% van de impact.

---

## Bronnen

- [GitHub Codespaces Prebuilds](https://docs.github.com/en/codespaces/prebuilding-your-codespaces/about-github-codespaces-prebuilds) — Officiële documentatie
- [GitHub Codespaces Pricing](https://github.com/pricing) — Free tier: 120 core hours/maand
- [Dev Container Specification](https://containers.dev/implementors/json_reference/) — JSON referentie
- [Facilitating Quick Creation](https://docs.github.com/en/codespaces/setting-up-your-project-for-codespaces/setting-up-your-repository/facilitating-quick-creation-and-resumption-of-codespaces) — Deep link parameters
- [DevPod](https://devpod.sh/) — Open source Codespaces alternatief
- [The 15-Minute Rule (Daily.dev)](https://business.daily.dev/resources/15-minute-rule-time-to-value-kpi-developer-growth) — TTFV benchmark
- [How to Write a 4000 Stars README (Daytona)](https://www.daytona.io/dotfiles/how-to-write-4000-stars-github-readme-for-your-project) — README-driven adoptie
- [Rust Dev Container](https://github.com/codespaces-examples/rust) — Officieel voorbeeld
- [Personalizing Codespaces](https://docs.github.com/en/codespaces/setting-your-user-preferences/personalizing-github-codespaces-for-your-account) — Dotfiles/themes
