# Field Presence Notes

Discovery run:

- date: 2026-04-25
- SDK: `openfeature-sdk==0.8.4`
- language: Python
- provider: SDK in-memory provider
- public call path: `client.get_boolean_details(...)`

## Raw returned detail shape

The returned object type was `openfeature.flag_evaluation.FlagEvaluationDetails`.

The raw object carried:

- `flag_key`
- `value`
- `variant`
- `flag_metadata`
- `reason`
- `error_code`
- `error_message`

The normal flag capture returned:

- `flag_key = "checkout.new_flow"`
- `value = true`
- `variant = "on"`
- `reason = "STATIC"`
- `flag_metadata = {}`
- no error code or message

The missing-flag fallback capture returned:

- `flag_key = "checkout.missing"`
- `value = false`
- `variant = null`
- `reason = "ERROR"`
- `error_code = "FLAG_NOT_FOUND"`
- `error_message = "Flag 'checkout.missing' not found"`
- `flag_metadata = {}`

## Reduction choices

The canonical fixture keeps:

- `flag_key`
- `result.value`
- `result.variant` when non-empty
- `result.reason` when non-empty
- `result.error_code` and `result.error_message` for the fallback case

The canonical fixture omits:

- raw `flag_metadata`, because discovery returned an empty object and provider
  metadata is default out of scope for P30
- caller-side `default_value`
- provider name and provider configuration
- the in-memory flag definition
- evaluation context, targeting key, hooks, telemetry, and transaction context

The fallback artifact is still a valid OpenFeature decision-detail artifact. It
means the SDK returned the default value with a bounded error reason and error
code, not that the sample infrastructure failed.
