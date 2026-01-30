# VCR cassettes (record/replay voor semantic/judge)

Deze directory bevat **geen echte API-responses** in de repo (geen secrets/PII). Cassettes worden lokaal opgenomen met `ASSAY_VCR_MODE=record` en een geldige API key; CI draait altijd **replay** (`ASSAY_VCR_MODE=replay`).

## Runtime contract

| Env | Betekenis |
|-----|-----------|
| `ASSAY_VCR_MODE` | `replay` (default in CI), `record` (lokaal), `off` (live netwerk) |
| `ASSAY_VCR_DIR` | Pad naar cassette-root (default: deze directory) |

- **CI:** altijd `replay`; outbound netwerk mag uit (geen flakiness).
- **Lokaal record:** `ASSAY_VCR_MODE=record` + API key; responses worden weggeschreven naar `ASSAY_VCR_DIR`. Scrub secrets vóór commit (of commit geen cassettes, alleen structuur).
- **Matching:** method + url + body (gecanonicaliseerde JSON); niet op Authorization-header. Zie PERFORMANCE-ASSESSMENT.md “VCR hygiene”.

## Structuur (na eerste record)

- `embeddings/` — opgenomen responses voor embedding-API (bijv. OpenAI /v1/embeddings).
- `judge/` — opgenomen responses voor judge/LLM-calls.

Zodra VCR-middleware in de code zit: record één keer lokaal, scrub indien nodig, commit cassettes zodat CI replay kan draaien. Zonder middleware blijft deze directory leeg; eval + trace zijn wel bruikbaar zodra replay geïmplementeerd is.
