# SPLIT MOVE MAP - Wave 52 LiveKit Tool Action Step1

## Step1 Movement

No production modules move in Step1.

## Source Hotspot

- `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs`
  (`1104` LOC on `origin/main @ 057151c5`)

## Current Internal Regions

- CLI args and command facade: `LiveKitToolActionArgs`,
  `cmd_livekit_tool_action`
- input loading and JSON/JSONL parsing: `read_livekit_tool_actions`,
  `parse_input_documents`
- receipt reduction: `reduce_tool_action_event`, `build_receipt`,
  `paired_call_outputs`
- schema/key validation: `validate_top_level`, `validate_call_keys`,
  `validate_output_keys`
- bounded value/timestamp validation: string, bool, timestamp helpers
- hashing/canonicalization: `hash_or_ref`, `sha256_json_value`,
  `canonical_json`, `sha256_file`
- importer tests: inline `mod tests`

## Step2 Candidate Moves

- `input.rs`: input document parsing and file digest helpers
- `reduce.rs`: event reduction, receipt construction, and list-order pairing
- `validate.rs`: key allowlists, forbidden-key checks, bounded strings,
  booleans, timestamps
- `canonical.rs`: canonical JSON and hash/ref helpers
- `bundle.rs`: command orchestration from events to `BundleWriter`
- `tests.rs`: existing importer tests after behavior stays frozen

## Non-Moves

- Do not alter CLI argument names or aliases.
- Do not change event or receipt schema strings.
- Do not alter Trust Basis classifier behavior.
- Do not import raw LiveKit session, transcript, trace, telemetry, or
  model/user payloads.
