# Overdracht: lokaal verifiëren of `sydneyvb-nl/tellur` echt werkt

**Doel:** empirisch vaststellen *in hoeverre* Tellur (AI code-provenance tool) daadwerkelijk
functioneert — niet alleen "bouwt het", maar "doet capture → attributie → verify → policy →
export wat het belooft?". Deze overdracht is geschreven op basis van een **read-only scout** van
de repo (README, docs, crate-structuur, bronfragmenten); de exacte CLI-flags zijn afgeleid en
moeten via `--help` bevestigd worden.

> **Belangrijk vooraf:** dit is pre-release software (v0.1.0, 13-07-2026). Een eerdere
> security-audit vond o.a. een daemon zonder auth (SEC-2) en een installer zonder checksum
> (SEC-3), inmiddels geremedieerd. **Bouw daarom uit source; gebruik NIET blind de
> `curl | bash` installer.** Draai in een wegwerp-VM/container, want Tellur installeert
> globale hooks in `~/.claude`, `~/.codex`, editor-configs etc.

---

## 0. Wat we al weten (uit de scout) — verwachtingen om te toetsen

| Claim uit repo | Te verifiëren |
|---|---|
| CLI-commando's: `explain`, `blame`, `sessions`, `verify`, `pr-report`, `policy check`, `export`, `setup`, `hooks ingest` | Bestaan ze? Werken ze? |
| Capture via Claude Code / Codex hooks (Tier 1), editor-extensies (Tier 3), imports (Tier 6) | Wordt een echte AI-edit vastgelegd? |
| SHA-256 hash chain per sessie, `verify` detecteert tampering | Faalt `verify` na handmatige mutatie? |
| YAML-policy: sensitive paths, `origin=Ai` + `tests_run`/`reviewer` eisen | Genereert `policy check` findings? |
| Export naar JSON / Agent Trace / SLSA v1.0 / SPDX 2.3 | Valideren de outputs tegen hun schema? |
| 61 unit tests, CI groen | `cargo test` groen? |
| Attributie-engine outsourcet `origin` naar de caller | Wat gebeurt er zonder adapter-bewijs? (→ `Unknown`?) |

---

## 1. Omgeving & prerequisites

```bash
# Wegwerp-omgeving aanbevolen (container of VM). Vereist:
rustc --version      # stable Rust toolchain (repo is 83% Rust)
cargo --version
git --version
node --version       # alleen nodig voor de VS Code-extensie (editor/)
# JetBrains-plugin vereist JDK/Gradle (Kotlin) — optioneel

git clone https://github.com/sydneyvb-nl/tellur.git
cd tellur
```

## 2. Build + test suite (fundament)

```bash
# Bouw de hele workspace
cargo build --workspace --release            # verwacht: schoon build, binaries in target/release
cargo build -p tellur-cli --release          # als de CLI-crate anders heet: check crates/cli/Cargo.toml [package] name

# Draai de test suite (claim: 61 unit tests)
cargo test --workspace                        # NOTEER: aantal passed/failed, duur
cargo clippy --workspace --all-targets        # kwaliteits-signaal
```

**Vastleggen:** exacte binary-naam (`target/release/<naam>`), aantal tests, eventuele
compile-warnings/errors. Zet die binary op PATH of gebruik het volledige pad hieronder als `tellur`.

## 3. CLI-oppervlak inventariseren (grond-waarheid vóór aannames)

```bash
tellur --help
tellur --version
# Elk subcommando dat de README noemt, expliciet checken:
for c in setup explain blame sessions verify pr-report policy export hooks connect inspect repo serve; do
  echo "=== $c ==="; tellur $c --help 2>&1 | head -40; echo;
done
```

**Vastleggen:** welke subcommando's echt bestaan en welke flags ze nemen. Alles hieronder
is een *hypothese* over syntax — corrigeer op basis van deze `--help`-output.

## 4. End-to-end functionele test (de kern: werkt capture → attributie?)

### 4a. Sandbox-repo opzetten
```bash
mkdir /tmp/tellur-demo && cd /tmp/tellur-demo && git init
printf 'fn main() {\n    println!("hello");\n}\n' > src/main.rs
git add -A && git commit -m "human baseline"
```

### 4b. Tellur activeren in de repo
```bash
tellur setup            # of: tellur repo init / tellur connect — check stap 3
ls -la .tellur/         # verwacht: config.yml, storage (SQLite?), refs
```

### 4c. Simuleer een AI-bijdrage via de capture-ingang
De sterkste capture-tier zijn lifecycle-hooks die `tellur hooks ingest --source <agent>` aanroepen
met een payload. Repliceer dat handmatig (dat is precies wat een Claude Code/Codex hook doet):
```bash
# Maak een "AI"-edit
printf 'fn main() {\n    println!("hello from AI");\n}\n' > src/main.rs

# Voer het door de ingest-pijplijn (payload-vorm afleiden uit docs/ADAPTERS.md + `hooks ingest --help`)
# Claude Code hook-payload bevat o.a. tool + file paths. Voorbeeld-hypothese:
echo '{"session_id":"test-1","source":"claude-code","tool":"Edit","file_paths":["src/main.rs"],"model":"claude-opus-4-8","prompt":"add AI greeting"}' \
  | tellur hooks ingest --source claude-code --auto-init
```
> Als de payload-vorm niet klopt: bekijk `docs/ADAPTERS.md` en de import-adapters in
> `crates/adapters/` voor exacte JSON-schema's. Er is ook een import-pad (JSONL/array/envelope).

### 4d. Attributie uitlezen
```bash
tellur blame src/main.rs           # verwacht: per-regel Ai/Human/Unknown + confidence
tellur explain src/main.rs:2       # verwacht: origin, model, session, evidence_strength
tellur sessions                    # verwacht: de zojuist gemaakte sessie met hash
```
**Toets deze verwachting:** regel 2 zou `origin=Ai`, `model=claude-opus-4-8`,
`evidence_strength=Recorded` (of `Claimed` als het als import binnenkwam) moeten tonen.
Regel 1/3 (ongewijzigd sinds human commit) → `Human` of `Unknown`.

> **Kritische testvraag:** de attributie-engine *outsourcet* de origin-beslissing naar de caller.
> Test daarom óók een edit **zónder** ingest (gewone `vi`-edit + commit): die moet als
> `Unknown`/`Human` verschijnen, niet als `Ai`. Als alles zonder bewijs toch "Ai" wordt, is de
> attributie onbetrouwbaar.

## 5. Integriteit / tamper-evidence (de belangrijkste security-claim)

```bash
tellur verify                       # verwacht: OK / chain intact

# Zoek de event-store en muteer één event met de hand
find .tellur -type f | head         # SQLite db of NDJSON-log?
# -> open de store, wijzig één payload-byte of één event_hash, sla op
tellur verify                       # VERWACHT: FAILT met tamper/rollback-detectie
```
**Dit is de make-or-break test.** Als `verify` een handmatige mutatie NIET detecteert, is de
kern-belofte (tamper-evident hash chain) niet waargemaakt. Test drie mutaties apart:
1. payload-byte wijzigen → moet falen (event_hash klopt niet meer)
2. laatste event verwijderen (truncation) → moet falen (head-hash checkpoint)
3. event toevoegen met verzonnen hash → moet falen (server/CLI hercomputeert)

## 6. Policy-engine

```bash
# Maak een policy (vorm afleiden uit crates/core/src/policy/ + docs/examples/)
cat > .tellur/policy.yml <<'YAML'
rules:
  - id: auth-needs-review
    paths: ["src/auth/**"]
    when: { attribution.origin: Ai }
    require: { reviewer_from_codeowners: true, tests_run: true }
    severity: high
YAML

mkdir -p src/auth && echo 'pub fn login() {}' > src/auth/mod.rs
# Attribueer dit als Ai via ingest (zie 4c), dan:
tellur policy check                 # verwacht: FAIL op auth-needs-review (geen review/tests)
echo "exit code: $?"                # verwacht non-zero
tellur pr-report --base main --head HEAD   # verwacht: risk-rapport met deze finding
```
**Vastleggen:** worden findings met severity gerapporteerd? Klopt de exit-code? Werkt
`block_ai_read` (secret-pad markeren en zien of capture het overslaat)?

## 7. Export + schema-validatie

```bash
tellur export --format json   > out.json
tellur export --format slsa   > out.slsa.json     # SLSA v1.0 provenance
tellur export --format spdx   > out.spdx.json     # SPDX 2.3
tellur export --format agent-trace > out.trace.json
```
**Toets:** valideer elke output tegen zijn publieke schema (`schemas/` in de repo, of officiële
SLSA/SPDX-schema's). Een export die niet valideert = claim niet waargemaakt.

## 8. Optioneel: Team Hub server + editor-extensie

```bash
# Server (FSL-licentie, crates/server): draai lokaal, test auth-gating
cargo run -p tellur-server --release   # of tellur serve — check stap 3
# Verwacht: loopback/HTTPS, tokens Argon2id-gehasht, self-registration geblokkeerd.
# Snelle check: ongeauthenticeerde POST naar ingest-endpoint moet 401/403 geven.

# VS Code-extensie: cd editor/<vscode-dir> && npm install && npm run build
# Installeer de .vsix in een wegwerp VS Code-profiel, sla een file op in een git-repo,
# check of er een sessie/event verschijnt via `tellur sessions`.
```

---

## 9. Beoordelingsrubriek — vul dit in als resultaat

| # | Test | Verwacht | Resultaat | Werkt? |
|---|------|----------|-----------|--------|
| 1 | `cargo build --workspace` | schoon | | ☐ |
| 2 | `cargo test --workspace` | ~61 tests groen | | ☐ |
| 3 | CLI-subcommando's bestaan | alle uit README | | ☐ |
| 4 | `setup` init `.tellur/` | config + store | | ☐ |
| 5 | AI-edit via `hooks ingest` gevangen | sessie + event | | ☐ |
| 6 | `blame`/`explain` toont Ai + model | correcte origin | | ☐ |
| 7 | Edit zónder bewijs → NIET "Ai" | Unknown/Human | | ☐ |
| 8 | `verify` OK op intacte store | pass | | ☐ |
| 9 | `verify` FAALT na byte-mutatie | fail | | ☐ |
| 10 | `verify` FAALT na truncation | fail | | ☐ |
| 11 | `policy check` genereert finding | fail + severity | | ☐ |
| 12 | `pr-report` risk-rapport | bevat findings | | ☐ |
| 13 | export valideert tegen SLSA/SPDX-schema | valide | | ☐ |
| 14 | Hub: unauth request geweigerd | 401/403 | | ☐ |
| 15 | Editor-extensie vangt file-save | event verschijnt | | ☐ |

**Eindoordeel formuleren als:** "X/15 kernclaims empirisch bevestigd. Sterk: … . Zwak/kapot: … .
Blokkers: … ." Onderscheid daarbij **bouwt** (triviaal) van **doet wat het belooft** (tests 5-13)
van **security-claims houden stand** (tests 7, 9, 10, 14) — dat laatste is het belangrijkst.

## 10. Valkuilen / let op
- Exacte CLI-syntax en payload-schema's zijn afgeleid, niet bevestigd — **altijd eerst `--help`
  en `docs/ADAPTERS.md`**.
- Tellur installeert globale hooks → draai geïsoleerd, ruim `~/.claude`/`~/.codex`-wijzigingen op.
- De crate-naam van de binary staat in `crates/cli/Cargo.toml` (`[[bin]]`/`[package] name`);
  gebruik die i.p.v. aan te nemen dat het `tellur` heet.
- Store-formaat (SQLite vs NDJSON) bepaalt hoe je in stap 5 muteert — inspecteer eerst.
- Netwerk-tests van de Hub tegen externe IdP/GitHub-App overslaan tenzij expliciet gewenst.

## 11. Referentie: wat de scout al vaststelde
- v0.1.0, CI groen (3 workflows), ~221 commits, zeer recent actief.
- Attributie-engine ~200 regels; confidence = `matched_lines/current_lines`.
- Schema: `TraceEvent{prev_hash,event_hash}`, `Origin{Human,Ai,Mixed,Unknown}`,
  `EvidenceStrength{Recorded,Imported,Inferred,Claimed,Unknown}`.
- Policy: YAML, sensitive paths + origin + review/test-eisen (geen OPA/Rego).
- Audit: 3 findings (High/Med/Low), alle geremedieerd; 61 unit tests.
