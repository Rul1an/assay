# assay policy

Policy authoring, validation, formatting, migration, activation, rollback, and status commands.

The policy family owns policy-authoring and lifecycle commands. The legacy top-level forms
`assay generate` and `assay record` were removed; use `assay policy generate`
and `assay policy record`.

---

## Synopsis

```bash
assay policy <COMMAND> [OPTIONS]
```

---

## Commands

| Command | Description |
|---------|-------------|
| [`assay policy generate`](generate.md) | Generate policy scaffolding from trace/profile input. |
| `assay policy record` | Capture runtime behavior and generate a policy. |
| `assay policy validate` | Validate policy syntax and v2 JSON Schemas. |
| `assay policy migrate` | Migrate v1.x constraints policies to v2.0 schemas. |
| `assay policy fmt` | Format policy YAML. |
| `assay policy resolve` | Dump the resolved policy this Assay version would load. |
| `assay policy activate` | Activate a validated policy into a policy root. |
| `assay policy rollback` | Roll back an active policy to its previously activated version. |
| `assay policy status` | Check active policy status and verify synchronization with activation history. |

---

## Examples

### Generate From A Trace

```bash
assay policy generate --input traces/session.jsonl --output policy.yaml
```

### Capture And Generate

```bash
assay policy record --output policy.yaml -- npm test
```

### Validate A Policy

```bash
assay policy validate --input policy.yaml
```

The default writes the human result to stderr and keeps stdout empty. For an
agent or CI caller, request the existing run-summary envelope explicitly:

```bash
assay policy validate --input policy.yaml --format json
```

Valid policies and malformed YAML write `assay.run_summary.v1` to stdout. A
YAML parse failure exits `2` with `reason_code: E_POLICY_PARSE` and a JSON-argv
`next_step`; a valid policy exits `0` with an empty reason code. Missing files,
semantic refusals, and schema-compile failures remain on the legacy stderr-only
path until they receive honest reason mappings. An empty stdout on those
failures is not a clean result; callers must still check the exit code. This
command reuses the existing summary envelope rather than introducing a new
schema; schema-identity evolution remains tracked in issue #2167.

---

### Resolve A Policy

```bash
assay policy resolve --input policy.yaml --format json
```

Success writes one `assay.policy.resolved.v0` document to stdout. The only
flags are `--input` and `--format`. JSON is the default; any other format
exits nonzero with empty stdout.

Exact success fields:

- `schema`: `assay.policy.resolved.v0`
- `canonicalization_profile`: `jcs:mcp_policy` (`POLICY_SNAPSHOT_CANONICALIZATION_JCS_MCP_POLICY`)
- `assay_version`: this CLI crate version
- `input_sha256`: SHA-256 of the bounded original bytes
- `policy_digest`: `McpPolicy::policy_digest()` over the loaded, normalized policy
- `policy`: the normalized policy object (`compiled` is omitted)

RFC8785/JCS of the emitted `policy` object, under that profile, hashes to
`policy_digest`. Consumers can reconstruct those bytes from `policy` +
`canonicalization_profile`. Whole-policy JCS is Vec-structural: reordered
object keys do not move the digest; permuting an allow-list does.

A YAML parse failure exits `2` with typed `E_POLICY_PARSE` on stderr and
empty stdout. Missing files and schema-compile failures stay on the honest
stderr-only path until they have a dedicated reason code. Do not treat an
empty stdout as success.

This dump is what this Assay version would load after successful
validation and schema compile. It does not claim that any runtime applied
the policy, that the policy is complete, safe, or compliant, or that the
producer is authenticated. The whole-policy digest is not the experimental
declared-constraint digest. The filename is not identity. Absence of a dump
is not a claim.

---

### Activate A Policy

```bash
assay policy activate src/policy.yaml --root /path/to/policy-root --as policy.yaml
```

Validates the input policy, stores the immutable content in `<root>/.assay/policy-store/<input_sha256>`,
atomically replaces `<root>/<name>` via atomic rename, and appends an activation record to
`<root>/.assay/activations/<NNNNNN>-<name>.json`.

Activation record schema (`assay.policy.activation.v0`):

- `schema`: `assay.policy.activation.v0`
- `name`: active policy filename within root (e.g. `policy.yaml`)
- `input_sha256`: SHA-256 of the raw policy bytes (`sha256:<hex>`)
- `policy_digest`: canonical `McpPolicy::policy_digest()`
- `previous_input_sha256`: previous active version's input hash, if any
- `previous_policy_digest`: previous active version's policy digest, if any
- `assay_version`: Assay CLI version
- `activated_at`: RFC3339 UTC timestamp
- `source`: source path or origin string
- `rollback_of`: optional pointer to previous activation record reversed by rollback

Concurrent activations on the same sequence number are resolved by bounded retries (up to 10 attempts).
If validation fails, the active policy file and activation records remain completely untouched (fail-closed).

---

### Roll Back A Policy

```bash
assay policy rollback policy.yaml --root /path/to/policy-root
```

Restores `<root>/<name>` to the bytes recorded in `previous_input_sha256` of the latest activation record.
The restored policy is fetched from `<root>/.assay/policy-store/<previous_input_sha256>`, validated,
atomically swapped into place, and recorded as a new activation record with `rollback_of` set to the
reversed record filename.

---

### Check Policy Status

```bash
assay policy status policy.yaml --root /path/to/policy-root [--format text|json]
```

Verifies that:
1. `<root>/<name>` exists and is valid policy YAML.
2. Its `input_sha256` and `policy_digest` match the latest recorded activation in `<root>/.assay/activations/`.
3. Its content is present in `<root>/.assay/policy-store/`.

Exits `0` when active bytes are verified and in sync with activation history. Exits non-zero (`2`) if
unrecorded active bytes or discrepancies are detected. In JSON mode, emits `assay.policy.status.v0`.

---

## Compatibility

- The legacy top-level `assay generate ...` and `assay record ...` paths were
  removed; use `assay policy generate ...` and `assay policy record ...`.
- Output shapes, exit codes, generated policy behavior, and policy schema
  semantics are unchanged.
