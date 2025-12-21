# Verdict Examples

Deze map bevat kleine, deterministische demo’s om Verdict te laten zien:
- **RAG Grounding**: detecteer hallucinaties + semantische drift
- **Negation Safety**: safety guardrails met must_contain + regex

Alle demo’s draaien **offline** met `--trace-file` (Replay Mode).

## Prereqs

Build de CLI:

```bash
cargo build --release
```

Gebruik vervolgens:

```bash
target/release/verdict --help
```

### Demo A: RAG Grounding

Pass case (grounded):

```bash
target/release/verdict ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/good.jsonl \
  --strict
```

Fail case (hallucination):

```bash
target/release/verdict ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/hallucination.jsonl \
  --strict
```

### Demo B: Negation Safety

Pass:

```bash
target/release/verdict ci \
  --config examples/negation-safety/eval.yaml \
  --trace-file examples/negation-safety/traces/safe-response.jsonl \
  --strict
```

Fail:

```bash
target/release/verdict ci \
  --config examples/negation-safety/eval.yaml \
  --trace-file examples/negation-safety/traces/unsafe-response.jsonl \
  --strict
```

### Baseline workflow (relative thresholds)

Maak baseline (op main):

```bash
target/release/verdict ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/good.jsonl \
  --export-baseline examples/rag-grounding/baseline.json \
  --strict
```

Gate PRs:

```bash
target/release/verdict ci \
  --config examples/rag-grounding/eval.yaml \
  --trace-file examples/rag-grounding/traces/good.jsonl \
  --baseline examples/rag-grounding/baseline.json \
  --strict
```
