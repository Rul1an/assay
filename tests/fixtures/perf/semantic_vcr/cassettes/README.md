# VCR cassettes (record/replay voor semantic/judge)

Deze directory bevat **geen echte API-responses** in de repo (geen secrets/PII). Cassettes worden lokaal opgenomen met `ASSAY_VCR_MODE=record` en een geldige API key; CI draait altijd **replay** (`ASSAY_VCR_MODE=replay`).

## VCR-middleware

Geïmplementeerd in `crates/assay-core/src/vcr/mod.rs`. Zie de module docs voor API.

## Runtime contract

| Env | Betekenis |
|-----|-----------|
| `ASSAY_VCR_MODE` | `replay` (default), `record` (lokaal), `off` (live netwerk) |
| `ASSAY_VCR_DIR` | Pad naar cassette-root (default: deze directory) |

- **CI:** altijd `replay`; outbound netwerk mag uit (geen flakiness).
- **Lokaal record:** `ASSAY_VCR_MODE=record` + API key; responses worden weggeschreven naar `ASSAY_VCR_DIR`. Scrub secrets vóór commit (of commit geen cassettes, alleen structuur).
- **Matching:** method + URL + body (SHA256 fingerprint van gecanonicaliseerde JSON); niet op Authorization-header. Zie PERFORMANCE-ASSESSMENT.md "VCR hygiene".

## Structuur (na eerste record)

- `embeddings/` — opgenomen responses voor embedding-API (bijv. OpenAI /v1/embeddings).
- `judge/` — opgenomen responses voor judge/LLM-calls.

## Cassette format

Elke cassette is een JSON-bestand met:

```json
{
  "method": "POST",
  "url": "https://api.openai.com/v1/embeddings",
  "request_body": {"input": "...", "model": "text-embedding-3-small"},
  "status": 200,
  "response_body": {"data": [{"embedding": [...]}]},
  "fingerprint": "abc123..."
}
```

Om cassettes op te nemen:

1. `export ASSAY_VCR_MODE=record`
2. `export OPENAI_API_KEY=sk-...`
3. Draai de test/bench die HTTP-requests maakt via `VcrClient`
4. Controleer cassettes, scrub indien nodig, commit

Zonder opgenomen cassettes faalt replay met "no cassette found for POST ...".
