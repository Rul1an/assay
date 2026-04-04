This folder contains a probe corpus for external signed receipt interop.

It is not a normative Assay schema.

The corrected valid fixtures use Passport envelope shape:

- `payload`
- `signature`

The malformed fixture intentionally stays malformed.

Only these fields are in scope for evaluation in this probe:

- `receipt_present`
- `verification_result`
- `issuer_id`
- `claimed_issuer_tier`
- `policy_digest`
- `spec`
- `tool_name`
- `decision`
- `session_id`
- `sequence`
- `tool_input_hash`
