# Negation Safety Example

This example demonstrates how to detect **Negation Blindness** (LLMs failing to see "not") using simple logic probes.

## Scenarios
1.  **Must Contain**: Validates that critical safety phrases are present (e.g. "DO NOT mix").
2.  **Metric**: regex match or keyword match.

## Usage
Run with the provided trace:

```bash
assay run --config eval.yaml --trace-file traces/safe-response.jsonl
```
