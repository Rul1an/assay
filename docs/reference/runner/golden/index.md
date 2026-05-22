# Runner Artifact Golden Shapes

Internal golden-shape examples for Assay-Runner v0 artifacts.

These files are examples of canonical field sets and stable serialization
shape. They are not delegated proof artifacts and do not replace the
Linux/eBPF three-run determinism gates. Example values are illustrative unless
the artifact contract explicitly defines a field's allowed value vocabulary.

- [`observation-health-openai-agents-kernel-policy-v0.json`](observation-health-openai-agents-kernel-policy-v0.json)
- [`capability-surface-openai-agents-kernel-policy-v0.json`](capability-surface-openai-agents-kernel-policy-v0.json)
- [`correlation-report-openai-agents-kernel-policy-v0.json`](correlation-report-openai-agents-kernel-policy-v0.json)
- [`capability-diff-s5-idempotent-v0.json`](capability-diff-s5-idempotent-v0.json)
- [`cross-runtime-diff-s5-gemini-v0.json`](cross-runtime-diff-s5-gemini-v0.json)

The machine-readable schema for the v0 clean-output shape lives next to
the contract document, not in this golden directory:

- [`../schema/cross-runtime-diff-v0-clean.schema.json`](../schema/cross-runtime-diff-v0-clean.schema.json)
