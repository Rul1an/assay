# Scout: Tellur vs. Assay

Read-only scout van [`sydneyvb-nl/tellur`](https://github.com/sydneyvb-nl/tellur) —
een AI code-provenance tool — en vergelijking met Assay. Plus een overdracht om Tellur
lokaal te verifiëren.

## Bestanden

| Bestand | Doel |
|---|---|
| `tellur-test-handoff.md` | Overdracht: stap-voor-stap lokaal verifiëren of Tellur echt werkt (build → capture → attributie → verify → policy → export), met 15-punts beoordelingsrubriek. |
| `tellur-verify.sh` | Semi-geautomatiseerd script dat stappen 1–7 (+ verify/tamper/policy/export) afdraait en een ingevulde `RUBRIC.md` schrijft. Draai in een **wegwerp-VM/container**. |

## Kernvergelijking (samenvatting)

Tellur en Assay zijn **complementair, geen concurrenten** — tegenovergestelde kanten van
de AI-code-levenscyclus:

| Dimensie | **Tellur** | **Assay** |
|---|---|---|
| Kernvraag | *Wie/welke AI schreef regel X?* (retrospectief) | *Mag deze agent-actie?* (proactief testen + afdwingen) |
| Wanneer | Ná het schrijven (provenance/audit) | Vóór (trace-replay tests) + tijdens (runtime) |
| Enforcement | Rapport/gate — informeert de mens | Tier 1 eBPF/LSM in de kernel + Tier 2 MCP-proxy |
| Integriteit | Lineaire SHA-256 hash chain | Merkle-root + RFC 8785 (JCS) canonicalisatie |
| Policy | YAML, smal (paden/origin/review-eisen), geen OPA/Rego | tool-args, sequences, regex, CIDR/poort |
| Capture | Breed: 6-tier adapter-hiërarchie (hooks→imports) | Smal: MCP-proxy + VCR replay, hoge zekerheid |
| Volwassenheid | v0.1.0, 4 crates, 61 tests, pre-release | v3.31.1, 21 crates, sim-harness, registry |

**Integratiekans:** Tellur levert *provenance* (wie schreef het, met welke bewijskracht) als
input voor Assay-policies; Assay evidence bundles kunnen andersom als `EvidenceStrength=Recorded`
(sterkste tier) in Tellur importeren. Ze delen genoeg DNA (Rust, MCP, hash-chain evidence,
declaratieve policy, "input is vijandig") voor een gedeeld canonicalisatie-/evidence-formaat.

> De inhoud is gebaseerd op een read-only scout (README, docs, crate-structuur, bronfragmenten);
> exacte CLI-flags/payload-schema's zijn afgeleid en moeten via `--help` + `docs/ADAPTERS.md`
> bevestigd worden bij het daadwerkelijk testen.
