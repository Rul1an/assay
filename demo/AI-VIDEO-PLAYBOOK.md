# AI-Video Productie Playbook: Assay Hero Demo

> **Format**: Terminal-hero + AI B-roll (Format A)
> **Doelvideo**: 35–45 seconden, 16:9
> **Narratief**: unsafe → FAIL → fix → PASS → compliance → CTA
> **Tone**: Understated, technisch, geen salesiness
> **Datum**: 2026-02-10

---

## Waarom Format A ("Terminal-hero + AI B-roll")

Terminal clips dragen de waarheid — echte CLI output, echte exit codes, echte kleuren. AI-video doet wat de terminal niet kan: abstracte concepten visueel maken (evidence chains, compliance, policy enforcement), overgangsshots genereren, en een "cinematic" gevoel toevoegen zonder de geloofwaardigheid te verliezen.

Regel: **als het in de terminal kan, film het in de terminal. Als het een concept is, laat AI het visualiseren.**

---

## Voiceover Script

**Totale duur:** ~38 seconden
**Tempo:** 150 wpm (rustig maar niet traag)
**Stijl:** Eerste persoon, droog, technisch, korte zinnen
**On-screen captions:** Altijd aan (80%+ kijkt zonder geluid)

```
[0.0s]  Your AI agent just called a tool it shouldn't have.
[3.0s]  Assay catches it. Exit code one. CI blocks the deploy.
[7.5s]  Fix the policy. Run it again.
[10.0s] Green. Deterministic. Same trace, same result, every time.
[15.0s] Every action your agent takes becomes a signed event
        in a content-addressed evidence bundle.
[21.0s] Merkle root, SHA-256, JCS canonicalization.
        Cryptographic proof of what your agent actually did.
[27.0s] Attack simulation tests your gates against known vectors.
        Bitflip, truncation, injection — blocked or bypassed, you'll know.
[33.0s] One install. No signup. Runs offline.
[36.0s] cargo install assay.
[38.0s] [stilte — CTA op scherm]
```

### Voiceover Productie

**Optie 1: Fish Audio / OpenAudio S1 (aanbevolen)**
- Open source, 50+ emoties, voice cloning met 10-30s audio
- Stel in op "calm" + "confident" emotie-tags
- Exporteer als WAV 48kHz

**Optie 2: ElevenLabs**
- "Daniel" of "Adam" stem (droog, technisch)
- Stability: 0.7, Similarity: 0.8, Style: 0.3

**Optie 3: Je eigen stem**
- Meest authentiek voor developer audience
- Neem op met Audacity/GarageBand, noise gate aan
- Mix op -14 LUFS (YouTube standaard)

---

## Shot List: 10 Shots

Elke shot heeft: doel, visueel, bron (terminal of AI), duur, en camera/motion.

---

### Shot 1 — "The Violation" (Hook)

| Veld | Waarde |
|------|--------|
| **Doel** | Spanning opbouwen — de agent doet iets fout |
| **Duur** | 0.0s – 3.0s (3 seconden) |
| **Bron** | Terminal (hero.tape, clip 1) |
| **Visueel** | Terminal op Catppuccin Mocha, `assay run` met unsafe trace, rode FAIL output verschijnt |
| **Voice** | "Your AI agent just called a tool it shouldn't have." |
| **Caption** | `POLICY VIOLATION DETECTED` |
| **Camera** | Static, full terminal frame |
| **Overgang** | Hard cut naar shot 2 |

**Terminal clip:** Knip uit hero.mp4 — het `assay run --config eval.yaml --trace-file traces/unsafe.jsonl` segment tot en met de rode FAIL output.

```bash
# Knip timestamp 0:00–0:03 uit hero.mp4
ffmpeg -i demo/output/hero.mp4 -ss 0 -t 3 -c copy shot01_violation.mp4
```

---

### Shot 2 — "CI Blocks" (Consequence)

| Veld | Waarde |
|------|--------|
| **Doel** | Laat zien dat CI het blokkeert — dit is niet alleen een waarschuwing |
| **Duur** | 3.0s – 5.0s (2 seconden) |
| **Bron** | AI B-roll |
| **Visueel** | Close-up van een CI pipeline (GitHub Actions-achtig) met een rode "blocked" status, subtiele glitch-effect bij transitie |
| **Voice** | "Assay catches it. Exit code one. CI blocks the deploy." |
| **Caption** | `exit code 1 · deploy blocked` |
| **Camera** | Slow push-in, handheld micro-shake |

**AI Prompt (Runway/Sora/Kling):**
```
Close-up of a dark-themed CI/CD pipeline dashboard showing a deployment
being blocked. A red status indicator pulses next to "Deploy to Production"
with text "BLOCKED — policy violation". Dark workspace, single monitor glow,
shallow depth of field. Camera: slow push-in with slight handheld movement.
Style: clean developer UI, high contrast, moody lighting, no readable code.
Duration: 2 seconds. Aspect ratio: 16:9.
```

---

### Shot 3 — "The Fix" (Transition)

| Veld | Waarde |
|------|--------|
| **Doel** | Toon de oplossing — simpel, snel, geen drama |
| **Duur** | 5.0s – 7.5s (2.5 seconden) |
| **Bron** | AI B-roll |
| **Visueel** | Handen op keyboard, YAML config wordt aangepast, één regel verandert van rood naar groen highlight |
| **Voice** | "Fix the policy. Run it again." |
| **Caption** | `policy.yaml · 1 line changed` |
| **Camera** | Over-shoulder, rack focus van scherm naar handen en terug |

**AI Prompt:**
```
Over-the-shoulder shot of a developer editing a YAML configuration file
on a dark-themed code editor. A single line is highlighted, transitioning
from red to green. Minimal desk setup, mechanical keyboard, dark room
with monitor as primary light source. Camera: rack focus from screen to
hands, then back. Style: cinematic, shallow depth of field, warm monitor
glow on face. Duration: 2.5 seconds. Aspect ratio: 16:9.
```

---

### Shot 4 — "Green Light" (Resolution)

| Veld | Waarde |
|------|--------|
| **Doel** | Opluchting — het werkt, groen, PASS |
| **Duur** | 7.5s – 10.0s (2.5 seconden) |
| **Bron** | Terminal (hero.tape, clip 2) |
| **Visueel** | Terminal met `assay run` + safe trace, groene PASS output, exit code 0 |
| **Voice** | "Green. Deterministic. Same trace, same result, every time." |
| **Caption** | `PASS · exit 0` |
| **Camera** | Static, full terminal frame |
| **Overgang** | Crossfade (0.3s) naar shot 5 |

**Terminal clip:** Knip uit hero.mp4 — het safe run segment.

```bash
ffmpeg -i demo/output/hero.mp4 -ss 3.5 -t 2.5 -c copy shot04_green.mp4
```

---

### Shot 5 — "Evidence Chain" (Concept Visual)

| Veld | Waarde |
|------|--------|
| **Doel** | Visualiseer het abstracte concept: elke actie wordt een signed event |
| **Duur** | 10.0s – 15.0s (5 seconden) |
| **Bron** | AI B-roll |
| **Visueel** | Abstract data visualization — events verschijnen als nodes in een chain/graph, elk met een hash, verbonden door lijnen. Merkle tree groeit van onder naar boven. |
| **Voice** | "Every action your agent takes becomes a signed event in a content-addressed evidence bundle." |
| **Caption** | `content-addressed · tamper-evident · auditable` |
| **Camera** | Slow dolly out, onthullend de volledige chain |

**AI Prompt:**
```
Abstract data visualization on dark background: glowing nodes appearing
one by one in a vertical chain structure, each node showing a short
hexadecimal hash. Thin luminous lines connect nodes upward into a
tree structure (Merkle tree). Nodes pulse briefly on appearance.
Color palette: deep navy background, cyan/teal node glow, white text.
Camera: slow dolly out revealing the full tree structure from bottom
to top. Style: minimal, technical, clean geometry, no text overlays
except hashes. Duration: 5 seconds. Aspect ratio: 16:9.
```

---

### Shot 6 — "Crypto Proof" (Technical Authority)

| Veld | Waarde |
|------|--------|
| **Doel** | Technische geloofwaardigheid — specifieke crypto primitives noemen |
| **Duur** | 15.0s – 21.0s (6 seconden) |
| **Bron** | Terminal (evidence-lint.tape output) + text overlay |
| **Visueel** | Terminal output van `assay validate` met bundle details, SHA-256 hashes zichtbaar |
| **Voice** | "Merkle root, SHA-256, JCS canonicalization. Cryptographic proof of what your agent actually did." |
| **Caption** | `SHA-256 · JCS · Merkle root` |
| **Camera** | Static terminal, subtle zoom op hash output |

**Terminal clip:** Knip uit evidence-lint.mp4.

```bash
ffmpeg -i demo/output/evidence-lint.mp4 -ss 0.5 -t 6 -c copy shot06_crypto.mp4
```

---

### Shot 7 — "Attack Sim" (Wow-factor)

| Veld | Waarde |
|------|--------|
| **Doel** | Visueel indrukwekkend — tabel met attack vectors |
| **Duur** | 21.0s – 27.0s (6 seconden) |
| **Bron** | Terminal (sim.tape output) |
| **Visueel** | ASCII tabel met attack vectors: bitflip, truncation, injection — "Blocked" / "Bypassed" status per vector |
| **Voice** | "Attack simulation tests your gates against known vectors. Bitflip, truncation, injection — blocked or bypassed, you'll know." |
| **Caption** | `sim run · 7 vectors · 0 bypassed` |
| **Camera** | Static, full terminal |

**Terminal clip:** Gebruik sim.mp4.

```bash
ffmpeg -i demo/output/sim.mp4 -ss 0.5 -t 6 -c copy shot07_sim.mp4
```

---

### Shot 8 — "Shield" (Transition B-roll)

| Veld | Waarde |
|------|--------|
| **Doel** | Visuele overgang naar CTA — "dit beschermt je" gevoel |
| **Duur** | 27.0s – 30.0s (3 seconden) |
| **Bron** | AI B-roll |
| **Visueel** | Abstract: een schild/barrier-achtige geometrische structuur die aanvallen absorbeert (subtiele particle effects), transformeert naar rust/stilte |
| **Voice** | — (muziek/ambient) |
| **Caption** | — |
| **Camera** | Pull back, stabiliserend |

**AI Prompt:**
```
Abstract geometric shield structure made of translucent hexagonal panels,
absorbing incoming particle streams. Particles dissolve on contact with
shield surface, creating brief ripple effects. Shield stabilizes and
glows steady. Dark background, teal and white color palette.
Camera: slow pull-back revealing full shield. Style: minimal, technical,
no organic elements, clean geometry. Duration: 3 seconds. Aspect ratio: 16:9.
```

---

### Shot 9 — "Value Props" (Setup CTA)

| Veld | Waarde |
|------|--------|
| **Doel** | Drie key messages in snelle successie |
| **Duur** | 30.0s – 36.0s (6 seconden) |
| **Bron** | Motion graphics (After Effects / CapCut / Remotion) |
| **Visueel** | Drie tekst-cards die snel achter elkaar invliegen: "One install." → "No signup." → "Runs offline." → dan: `cargo install assay` met cursor |
| **Voice** | "One install. No signup. Runs offline. cargo install assay." |
| **Caption** | — (text IS de visual) |
| **Camera** | Static |

**Productie:** Dit is een motion graphics shot, geen AI-video. Maak met:
- **Remotion** (React-based, code-driven — past bij jullie stack)
- **CapCut** (snelste optie)
- **ffmpeg drawtext filter** (minimale dependencies)

```bash
# Minimale versie met ffmpeg (3 text cards + final command)
ffmpeg -f lavfi -i color=c=1E1E2E:s=1280x720:d=6 \
  -vf "drawtext=text='One install.':fontcolor=white:fontsize=48:\
       x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,0,1.5)',\
       drawtext=text='No signup.':fontcolor=white:fontsize=48:\
       x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,1.5,3)',\
       drawtext=text='Runs offline.':fontcolor=white:fontsize=48:\
       x=(w-text_w)/2:y=(h-text_h)/2:enable='between(t,3,4.5)',\
       drawtext=text='cargo install assay':fontcolor=89dceb:\
       fontsize=36:x=(w-text_w)/2:y=(h-text_h)/2:\
       enable='between(t,4.5,6)'" \
  -c:v libx264 -pix_fmt yuv420p shot09_cta.mp4
```

---

### Shot 10 — "End Card" (CTA)

| Veld | Waarde |
|------|--------|
| **Doel** | Call to action — waar gaan ze heen |
| **Duur** | 36.0s – 40.0s (4 seconden) |
| **Bron** | Static frame (design) |
| **Visueel** | Assay logo (of wordmark) + `github.com/...` + `Try in Codespaces →` |
| **Voice** | — (stilte of ambient fade-out) |
| **Caption** | — |
| **Camera** | Static, 3s hold, fade to black |

**Productie:** Maak als PNG in Figma/Canva, converteer naar video:

```bash
ffmpeg -loop 1 -i end_card.png -t 4 -c:v libx264 \
  -pix_fmt yuv420p -vf "fade=t=out:st=3:d=1" shot10_end.mp4
```

---

## Samenstelling: Final Cut

### Methode 1: ffmpeg concat (snel, geen GUI)

```bash
# 1. Maak concat lijst
cat > concat.txt << 'EOF'
file 'shot01_violation.mp4'
file 'shot02_ci_blocks.mp4'
file 'shot03_fix.mp4'
file 'shot04_green.mp4'
file 'shot05_evidence.mp4'
file 'shot06_crypto.mp4'
file 'shot07_sim.mp4'
file 'shot08_shield.mp4'
file 'shot09_cta.mp4'
file 'shot10_end.mp4'
EOF

# 2. Concat (alle shots moeten zelfde resolutie/codec hebben)
ffmpeg -f concat -safe 0 -i concat.txt -c copy assay_hero_raw.mp4

# 3. Voeg voiceover toe
ffmpeg -i assay_hero_raw.mp4 -i voiceover.wav \
  -c:v copy -c:a aac -b:a 192k \
  -shortest assay_hero_final.mp4
```

### Methode 2: DaVinci Resolve (gratis, meer controle)

1. Import alle shots op timeline
2. Voeg crossfades toe (0.2-0.3s) bij shots 4→5 en 8→9
3. Drop voiceover op audio track
4. Voeg captions toe als subtitles (SRT of burn-in)
5. Export: H.264, 1080p, 8 Mbps, AAC 192kbps

### Methode 3: Remotion (code-driven, CI-friendly)

Als je de video reproduceerbaar wilt houden (past bij jullie "demo as code" filosofie):

```tsx
// video/src/AssayHero.tsx
import { Composition, Sequence, Video, Img } from 'remotion';

export const AssayHero = () => (
  <>
    <Sequence from={0} durationInFrames={90}>   {/* Shot 1: 3s @ 30fps */}
      <Video src="/shots/shot01_violation.mp4" />
    </Sequence>
    <Sequence from={90} durationInFrames={60}>   {/* Shot 2: 2s */}
      <Video src="/shots/shot02_ci_blocks.mp4" />
    </Sequence>
    {/* ... etc */}
  </>
);
```

Voordeel: video wordt gegenereerd via `npx remotion render`, versioneerbaar in git, CI-automatiseerbaar.

---

## Platform-Specifieke Exports

Na de master cut, exporteer varianten:

| Platform | Resolutie | Aspect | Max duur | Captions | Bestand |
|----------|-----------|--------|----------|----------|---------|
| GitHub README | n/a | n/a | n/a | Nee | hero.gif (kort, 6-8s) |
| Landing page | 1920x1080 | 16:9 | ∞ (loop) | Nee | hero.mp4 (muted autoplay) |
| YouTube | 1920x1080 | 16:9 | 38s | SRT | assay_hero_final.mp4 |
| X/Twitter | 1280x720 | 16:9 | 2m20s | Burn-in | assay_hero_twitter.mp4 |
| LinkedIn | 1920x1080 | 16:9 | 10m | Burn-in | assay_hero_linkedin.mp4 |
| Instagram/TikTok | 1080x1920 | 9:16 | 60s | Burn-in | assay_hero_vertical.mp4 |
| Reddit | 1280x720 | 16:9 | — | Burn-in | assay_hero_reddit.mp4 |

### Verticale versie (9:16) voor Instagram/TikTok

```bash
# Crop 16:9 naar 9:16 met terminal in center
ffmpeg -i assay_hero_final.mp4 \
  -vf "crop=ih*9/16:ih,scale=1080:1920" \
  -c:a copy assay_hero_vertical.mp4
```

### Twitter-optimized (max 512MB, max 2:20)

```bash
# Re-encode voor Twitter's codec eisen
ffmpeg -i assay_hero_final.mp4 \
  -c:v libx264 -preset slow -crf 23 \
  -c:a aac -b:a 128k \
  -movflags +faststart \
  assay_hero_twitter.mp4
```


---

## Google Veo Strategy (Premier AI Video)

Anno 2026 is **Google Veo** de gouden standaard voor developer marketing video's. Het model blinkt uit in "physics-aware" rendering van UI elementen en abstracte data structuren.

### Hybride Workflow: Terminal + Veo
Je kunt de standaard terminal-simulaties vervangen door Veo-generaties. Plaats MP4 bestanden in `demo/assets/overrides/shotXX.mp4`.

#### Shot 2: CI Blocked (Image-to-Video)
1. **Input**: Screenshot van GitHub Actions "failed" state (rood).
2. **Prompt**: "Cinematic slow zoom into the red failure icon. Dark mode UI, slight chromatic aberration, digital noise in background. High tech, ominous atmosphere."
3. **Motion**: Pan/Zoom specificatie (Veo controls).

#### Shot 5: Evidence Chain (Text-to-Video)
1. **Prompt**: "Glowing merkle tree nodes connecting vertically in a dark void. Cybernetic roots extending downwards. Nodes pulse with cyan light when connected. Unreal Engine 5 render style, 8k, macro lens."
2. **Style**: "Technical, Abstract, Cyberpunk but clean".

#### Shot 8: Shield (Text-to-Video)
1. **Prompt**: "Abstract glass hexagonal shield deflecting digital particles. Particles dissolve upon impact. Slow motion, shallow depth of field, cool blue lighting. 3D render."

---

## SOTA Voice 2026

Voor de "Understated Technical" tone of voice zijn dit de top keuzes:

1. **ElevenLabs Turbo v3**:
   - Voice: "Glitch" of "Marcus" (Deep, confident).
   - Settings: Stability 0.5, Clarity 0.9.
2. **Fish Audio S1 (Open Source)**:
   - Voice: `en-US-calm-male` (meegeleverd in install script).
   - Draait lokaal, geen kosten.
3. **Google Veo Audio**:
   - Kan ook *sound effects* genereren (glitches, typ-geluiden) gesynchroniseerd met video.

---

## AI B-Roll Prompting: Universele Regels

### Wat werkt

1. **Start simpel, voeg detail strategisch toe** — minder prompt = sneller itereren, minder "prompt spaghetti"
2. **Positieve taal** — beschrijf wat je wél ziet, niet wat je niet wilt
3. **Concrete visuele details** — "translucent hexagonal panels" niet "professional shield"
4. **Subject + Action + Camera + Style** — altijd deze vier in elke prompt
5. **Plan 5–15 generaties per bruikbare shot** — iteratie is normaal, niet falen

### Wat faalt

1. **Abstract woorden** — "professional", "mooi", "modern" zijn zinloos voor AI modellen
2. **Negatieve instructies** — "no text" werkt slechter dan gewoon niet noemen
3. **Teveel in één prompt** — max 1 actie per shot, max 3 seconden per generatie
4. **Leesbare tekst verwachten** — AI-modellen kunnen (nog) geen correcte tekst genereren in video
5. **Inconsistente stijl** — gebruik dezelfde kleurpalette-beschrijving in elke prompt

### Assay Stijlgids voor AI Prompts

Gebruik deze constanten in elke prompt voor visuele consistentie:

```
Kleurpalette: "deep navy/dark background (#1E1E2E), teal/cyan accents
(#89dceb), white text, no warm colors"

Belichting: "single monitor glow as primary light, dark room,
high contrast"

Stijl: "minimal, technical, clean geometry, developer aesthetic,
shallow depth of field"

Verboden: "no organic elements, no people's faces, no stock photo
aesthetic, no lens flares, no readable code/text"
```

---

## Narrate.sh Upgrade

Je huidige `narrate.sh` gebruikt macOS `say` — prima voor prototyping, maar niet voor release. Upgrade pad:

### Fish Audio (Open Source, SOTA)

```bash
# Zie demo/install_voice.sh voor setup
python3 -c "
from fish_audio_sdk import Session
session = Session('YOUR_API_KEY') # Of lokaal model
# ... implementation details ...
"
```

### ElevenLabs (Commercieel, Hoge Kwaliteit)

```bash
#!/bin/bash
# Requires: ELEVEN_API_KEY env var

curl -X POST "https://api.elevenlabs.io/v1/text-to-speech/pNInz6obpgDQGcFmaJgB" \
  -H "xi-api-key: $ELEVEN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "text": "Your AI agent just called a tool it should not have...",
    "voice_settings": {
      "stability": 0.7,
      "similarity_boost": 0.8,
      "style": 0.3
    }
  }' \
  --output demo/output/voiceover.mp3
```

---

## Captions: Burn-in vs SRT

### Burn-in (voor social media — altijd zichtbaar)

```bash
# Genereer SRT eerst, dan burn in
ffmpeg -i assay_hero_final.mp4 \
  -vf "subtitles=captions.srt:force_style='\
    FontName=JetBrains Mono,FontSize=20,\
    PrimaryColour=&Hffffff,OutlineColour=&H000000,\
    BorderStyle=3,Outline=2,Shadow=0,\
    MarginV=40,Alignment=2'" \
  -c:a copy assay_hero_burned.mp4
```

### SRT bestand

```srt
1
00:00:00,000 --> 00:00:03,000
Your AI agent just called
a tool it shouldn't have.

2
00:00:03,000 --> 00:00:07,500
Assay catches it. Exit code one.
CI blocks the deploy.

3
00:00:07,500 --> 00:00:10,000
Fix the policy. Run it again.

4
00:00:10,000 --> 00:00:15,000
Green. Deterministic. Same trace,
same result, every time.

5
00:00:15,000 --> 00:00:21,000
Every action becomes a signed event
in a content-addressed evidence bundle.

6
00:00:21,000 --> 00:00:27,000
Merkle root, SHA-256, JCS canonicalization.
Cryptographic proof of what your agent did.

7
00:00:27,000 --> 00:00:33,000
Attack simulation tests your gates.
Blocked or bypassed — you'll know.

8
00:00:33,000 --> 00:00:36,000
One install. No signup. Runs offline.

9
00:00:36,000 --> 00:00:40,000
cargo install assay
```

---

## QA Checklist (voor release)

- [ ] Elk terminal fragment toont echte Assay output (geen mock)
- [ ] Exit codes kloppen (1 voor fail, 0 voor pass)
- [ ] AI B-roll bevat geen leesbare tekst/code (AI hallucinatie risico)
- [ ] Voiceover claims matchen met wat de tool daadwerkelijk doet
- [ ] Captions zijn grammaticaal correct en gesynchroniseerd
- [ ] Geen superlatieven ("fastest", "best", "first", "only")
- [ ] `cargo install assay` werkt daadwerkelijk als je het uitvoert
- [ ] Video speelt correct op mobile (test op echte telefoon)
- [ ] Thumbnail (hero-thumb.png) werkt als standalone beeld
- [ ] Alle clips hebben consistent kleurprofiel (rec709)
- [ ] Audio is genormaliseerd op -14 LUFS
- [ ] Geen copyrighted muziek/sound effects

---

## Timing Synchronisatie: Voice × Terminal × AI

| Seconde | Shot | Bron | Voice | On-Screen |
|---------|------|------|-------|-----------|
| 0.0–3.0 | 1 | Terminal | "Your AI agent just called a tool it shouldn't have." | `assay run` → rode FAIL |
| 3.0–5.0 | 2 | AI | "Assay catches it. Exit code one. CI blocks the deploy." | CI dashboard, rode block |
| 5.0–7.5 | 3 | AI | "Fix the policy. Run it again." | Editor, YAML lijn rood→groen |
| 7.5–10.0 | 4 | Terminal | "Green. Deterministic. Same trace, same result, every time." | `assay run` → groene PASS |
| 10.0–15.0 | 5 | AI | "Every action...signed event...evidence bundle." | Merkle tree visualisatie |
| 15.0–21.0 | 6 | Terminal | "Merkle root, SHA-256, JCS canonicalization..." | `assay validate` output |
| 21.0–27.0 | 7 | Terminal | "Attack simulation...blocked or bypassed..." | Sim tabel output |
| 27.0–30.0 | 8 | AI | — (ambient) | Shield/barrier visual |
| 30.0–36.0 | 9 | Motion | "One install. No signup. Runs offline." | Text cards |
| 36.0–40.0 | 10 | Static | — (fade-out) | Logo + GitHub URL + CTA |

---

## Productiepad Samenvatting

```
1. Terminal clips knippen    → ffmpeg -ss/-t uit bestaande .mp4's
2. AI B-roll genereren       → 4 shots × 5-15 iteraties = 20-60 generaties
3. Voice-over opnemen/gen    → Fish Audio of eigen stem
4. Motion graphics (shot 9)  → ffmpeg drawtext of Remotion
5. End card (shot 10)        → Figma/Canva → ffmpeg
6. Assembly                  → ffmpeg concat of DaVinci Resolve
7. Captions                  → SRT + burn-in voor social
8. Platform exports          → 16:9, 9:16, Twitter-optimized
9. QA pass                   → Checklist hierboven
```

**Geschatte productietijd:** 4-6 uur voor eerste versie, 1-2 uur per iteratie daarna.
