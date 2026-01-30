# Semantic/judge VCR workload

Workload om **precompute_ms** en **cache-gedrag** voor embeddings en judge te meten **zonder** netwerk/LLM-variance. SOTA: record/replay op HTTP-niveau (VCR).

## VCR-middleware

De VCR-middleware is geïmplementeerd in `crates/assay-core/src/vcr/mod.rs`:

- **`VcrClient`**: HTTP client wrapper met record/replay support
- **Fingerprinting**: SHA256 hash van method + URL + body (Authorization excluded)
- **Cassettes**: JSON-bestanden in `cassettes/{embeddings,judge}/`

## Gebruik

- **Replay (CI, geen secrets):** `ASSAY_VCR_MODE=replay` (default). Geen outbound netwerk; responses uit cassettes.
- **Record (lokaal):** `ASSAY_VCR_MODE=record` + API key. Responses worden weggeschreven naar `ASSAY_VCR_DIR` (default: `tests/fixtures/perf/semantic_vcr/cassettes`). Scrub secrets vóór commit.
- **Off:** `ASSAY_VCR_MODE=off` — live netwerk (niet in CI).

```rust
use assay_core::vcr::{VcrClient, VcrMode};

// From environment
let mut client = VcrClient::from_env();

// Or explicit
let mut client = VcrClient::new(VcrMode::Replay, PathBuf::from("cassettes"));

// HTTP request met VCR
let response = client.post_json(url, &body, Some(&auth_header)).await?;
```

Zie `cassettes/README.md` voor env-contract en matching rules.

## Bestanden

| Bestand | Doel |
|---------|------|
| `eval_semantic_vcr.yaml` | 1× semantic_similarity_to (embedding), 1× faithfulness (judge) |
| `trace_semantic_vcr.jsonl` | Episodes die bij de tests horen |
| `cassettes/` | Recorded HTTP responses (embeddings/, judge/) — leeg tot eerste record |

## CI

CI draait **alleen replay**; record nooit in CI. Zie PERFORMANCE-ASSESSMENT.md "Semantic/judge VCR-workload".
