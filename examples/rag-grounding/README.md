# RAG Grounding Example

This example demonstrates how to evaluate Retrieval Augmented Generation (RAG) pipelines for **Grounding** (Faithfulness) and **Semantic Similarity**.

## Scenarios
1.  **Semantic Similarity**: Does the answer match the golden reference meaning?
2.  **Must Contain**: Does the answer contain specific key terms (e.g. "€385")?
3.  **Must Not Contain**: Does the answer hallucinate terms (e.g. "€500")?

## Usage
Run with the provided trace (no API key required):

```bash
verdict run --config eval.yaml --trace-file traces/good.jsonl
```
