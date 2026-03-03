# Sink Failure Variant fixtures

Step2 uses env-controlled deterministic sink outcomes:

- `SINK_PRIMARY_OUTCOME=ok|timeout|partial`
- `SINK_ALT_OUTCOME=ok|timeout|partial`

No additional payload fixtures are required for Step2: this slice is wiring plus deterministic outcome semantics.
