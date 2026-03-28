# Assay Python Bindings

`assay-it` is the Python package for the `assay` import namespace.

It currently provides a small, bounded surface on top of Assay core:

- `AssayClient` for writing JSONL trace records from Python
- `Coverage` for policy coverage analysis against trace/tool-call inputs
- `Explainer` for rule-by-rule explanation of a single trace
- a `pytest` fixture plugin exposed as `assay_client`

## Install

```bash
pip install assay-it
```

Import path:

```python
from assay import AssayClient, Coverage, Explainer, validate
```

## What It Is

This package is for Python-side trace capture and lightweight policy-analysis helpers.
It is useful when you want to:

- append structured trace events to a JSONL file from Python
- analyze tool-call traces against an Assay policy
- explain why a given trace matched or violated policy rules
- use a simple pytest fixture around the Python client

## What It Is Not

This package is not the full Assay CLI or the full trust-compiler surface.
It does **not** replace:

- `assay mcp wrap` and runtime MCP enforcement
- evidence bundle generation and verification workflows
- Trust Basis / Trust Card generation
- the GitHub Action or release-binary install path

Those outward-facing product surfaces live in the main Assay CLI and repository docs.

## Example

```python
from assay import AssayClient, Coverage, Explainer

client = AssayClient("traces.jsonl")
client.record_trace({"tool": "ToolA", "args": {"q": "weather"}})

coverage = Coverage("assay.yaml")
report = coverage.analyze([[{"tool": "ToolA", "args": {"q": "weather"}}]])

explainer = Explainer("assay.yaml")
details = explainer.explain([{"tool": "ToolA", "args": {"q": "weather"}}])
```

## Repository

- Main project: <https://github.com/Rul1an/assay>
- Current CLI and trust-compiler docs: <https://github.com/Rul1an/assay/blob/main/README.md>
