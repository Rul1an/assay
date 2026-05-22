# Gemini Identity Probe Results

This directory holds committed outcome files from the Gemini identity
preservation probe.

Each probe run writes one JSON file here named
`identity-probe-<UTC-timestamp>.json` with schema
`assay.runner.gemini_identity_probe.v0`.

The outcome file is the durable record that the level-3 stable-identity
assumption from
[`#1305`](https://github.com/Rul1an/assay/pull/1305) was verified by an
actual record-and-replay against `gemini-3.5-flash`. Without a passing
outcome here, the fixture implementation PR cannot proceed per
[issue #1307](https://github.com/Rul1an/assay/issues/1307).

## Outcome JSON shape

```json
{
  "schema": "assay.runner.gemini_identity_probe.v0",
  "mode": "record" | "replay" | "record-and-replay",
  "timestamp_utc": "<ISO-8601>",
  "model_pin": "gemini-3.5-flash",
  "function_call": {
    "id": "<the FunctionCall.id observed>",
    "name": "read_file",
    "args_keys": ["path"]
  } | null,
  "error": "<message>" | null,
  "passed": true | false
}
```

## What counts as a passing outcome

For the implementation PR to proceed, at minimum one committed file here
must have:

- `mode = "record-and-replay"` (or one passing `record` plus one passing
  `replay` with matching `function_call.id`)
- `function_call.id` present, non-empty, and identical between record and
  replay modes
- `passed = true`
- `error = null`
- `model_pin = "gemini-3.5-flash"`

If any committed outcome has `passed = false`, do **not** proceed with
the fixture implementation. See `MAINTAINER-PROBE.md` for the kill-criteria
escalation procedure.

## Retention

Outcome files are kept indefinitely. They form an audit trail of how the
identity guarantee held over time and across `google-genai` SDK bumps,
Gemini model updates, or cassette re-recordings.

Re-running the probe (after a model or SDK bump per the bump flow in
[`fixtures-v0.md` § Dependency Upgrade Contract](../../../docs/reference/runner/fixtures-v0.md#dependency-upgrade-contract))
adds a new outcome file alongside the old one; it does not replace it.
