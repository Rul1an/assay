# Mutation-detection matrix

Threat model: post-hoc mutation by a party without the signing key; the run anchor (run_root) is bound by an external signature the attacker cannot forge

Base events: 64

| class | layer | detected | no-op | manifest-meta-only | bypass | dominant verifier code |
| --- | --- | --- | --- | --- | --- | --- |
| gzip_bitflip | internal-verifier | 447 | 0 | 1 | 0 | `ContractInvalidJson` |
| truncate | internal-verifier | 9 | 0 | 0 | 0 | `IntegrityIo` |
| inject_file | internal-verifier | 1 | 0 | 0 | 0 | `ContractUnexpectedFile` |
| inject_path_traversal | internal-verifier | 1 | 0 | 0 | 0 | `SecurityPathTraversal` |
| inject_absolute_path | internal-verifier | 1 | 0 | 0 | 0 | `SecurityPathTraversal` |
| event_byte_edit | internal-verifier | 32 | 0 | 0 | 0 | `ContractInvalidJson` |
| event_drop | internal-verifier | 1 | 0 | 0 | 0 | `IntegrityFileSizeMismatch` |
| event_reorder | internal-verifier | 1 | 0 | 0 | 0 | `ContractSequenceStart` |
| ndjson_bom | internal-verifier | 1 | 0 | 0 | 0 | `IntegrityFileSizeMismatch` |
| ndjson_crlf | internal-verifier | 1 | 0 | 0 | 0 | `IntegrityFileSizeMismatch` |
| tar_duplicate | internal-verifier | 1 | 0 | 0 | 0 | `ContractDuplicateFile` |
| consistent_rewrite | run-anchor | anchor-detected | - | - | - | run_root change |

## Gate

- event-evidence bypasses: **0**
- manifest-meta-only (documented limitation): 1

## Documented limitation

manifest metadata not referenced by a verifier check is not individually hash-bound (manifest_meta_only); the event evidence (events.ndjson) and the run anchor (run_root) are always hash-checked
