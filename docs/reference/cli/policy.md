# assay policy

Policy authoring, validation, formatting, migration, activation, rollback, status, and offline lookup commands.

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
| `assay policy lookup` | Find retained source bytes and committed activation records by semantic policy digest. |

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

Preview the same bounded validation and compare the proposed policy with the current active bytes
without creating `.assay`, storing content, replacing the active file, or appending a record:

```bash
assay policy activate src/policy.yaml --root /path/to/policy-root --as policy.yaml --dry-run
```

The text result keeps raw-byte change (`byte_change`) separate from normalized policy change
(`semantic_change`). A formatting-only rewrite can therefore report `byte_change=true` and
`semantic_change=false`. A first activation reports both as changed. The preview validates an
existing active file with the same 1 MB input ceiling and refuses invalid or symlinked targets.
It is an observation at one point in time: it reserves nothing and does not guarantee that a later
activation will see the same current bytes. It does not acquire the transaction lock or validate the
activation history, so a live activation can still refuse an unrecorded or inconsistent root. It
emits no JSON identity and writes no policy state.

The target name must be a portable 1–128-byte ASCII filename: letters, digits, `.`, `_` and `-`,
beginning with a letter or digit, not ending in `.`, and not a Windows device name. A name that
differs only by ASCII case from an existing target or activation history is refused.

Activation takes a root-scoped kernel lock, verifies the committed head, and rolls a pending
pointer forward only when the current bytes match that head's recorded predecessor. It stores
the validated bytes at `<root>/.assay/policy-store/sha256-<hex>`; the logical identity in records
remains `sha256:<hex>`. Existing POSIX store objects named `sha256:<hex>` remain readable. The
command then publishes `<root>/.assay/activations/<NNNNNN>-<name>.json` before atomically replacing
`<root>/<name>`, and verifies the active bytes before reporting success. The record publication is
the commit point. A process crash after that point can leave a committed record with the previous
active bytes; `status` refuses the mismatch and the next activation or rollback rolls forward
before continuing. A command that fails after record publication may therefore already have
committed; inspect `status` and the latest record before retrying.

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

Concurrent activations serialize on the root lock. A live lock holder causes a `busy` refusal after
at most ten seconds of lock waiting; it does not create a record or replace active bytes. Input
validation fails before touching policy state. Recovery refuses a malformed head, a broken immediate
predecessor link, a corrupt store object, or foreign active bytes rather than guessing. Record reads
are limited to 64 KiB, policy/store reads to 1 MB, directory scans to 10,000 entries each, and stale
transaction-temp cleanup to 64 files per operation. These limits do not bound operating-system
`fsync` or rename latency.

---

### Roll Back A Policy

```bash
assay policy rollback policy.yaml --root /path/to/policy-root
```

Restores `<root>/<name>` from the immediately preceding committed activation record, after
checking that its sequence and digests agree with the latest record. The stored bytes are bounded,
hashed and validated. Rollback then commits a new record with `rollback_of` set to the reversed
record filename and replaces the active file. The same lock and crash recovery rules apply.

---

### Check Policy Status

```bash
assay policy status policy.yaml --root /path/to/policy-root [--format text|json]
```

Verifies that:
1. `<root>/<name>` exists and is valid policy YAML.
2. Its `input_sha256` and `policy_digest` match the latest recorded activation in `<root>/.assay/activations/`.
3. The latest record's immediate predecessor link is consistent, and its content-addressed store
   object is present and hashes to its logical identity.

Exits `0` when active bytes are verified and in sync with activation history. Exits non-zero (`2`) if
unrecorded active bytes or discrepancies are detected. In JSON mode, emits `assay.policy.status.v0`.

The transaction guarantees here concern process crashes on a local filesystem. POSIX record and
pointer publication sync their directories; freshly created parent directories and Windows
directory entries have no cross-platform power-loss durability claim. Network filesystems with
different lock or rename semantics are unsupported. On Windows, a reader that prevents replacement
can cause a bounded rename retry and then a non-zero, committed-pointer-pending outcome.

---

### Look Up Retained Policy Bytes

```bash
assay policy lookup sha256:<64-lowercase-hex> --root /path/to/policy-root --format json
assay policy lookup sha256:<64-lowercase-hex> --root /path/to/policy-root \
  --input-sha256 sha256:<64-lowercase-hex> --output recovered.yaml
```

Lookup is offline and does not alter the policy root. It accepts only canonical lowercase digest
arguments before accessing that root. On success it writes one `assay.policy.lookup.v0` JSON
document to stdout (`--format text` prints a compact listing). `matches` contains every distinct
raw-byte identity whose bounded stored bytes hash to their store identity and whose loaded policy
has the requested semantic digest. Matches are ordered by `input_sha256`. Each match gives the
byte length, logical `store_object` identity, locator filenames, `recorded`, and all committed
activation records citing that exact input/semantic pair. `recorded: false` means retained bytes
without a committed activation, not an inferred activation. Histories remain separate when two
different byte representations have the same semantic digest.

`--output` writes the selected verified bytes through a temporary file and rename. An explicit
`--input-sha256` selects one raw-byte identity; without it, export refuses zero or multiple matches
rather than choosing the first. The output cannot replace files under the policy root's `.assay`
metadata directory. A lookup or export error is nonzero and produces no complete JSON document;
an existing output file is retained on errors before the final rename.

Object reads are capped at 1,000,000 bytes, activation records at 64 KiB, and each directory at
10,000 entries. One scan examines at most 64 MiB; at most three scans and 192 MiB of aggregate
input are allowed. A shared read lock on the existing transaction lock excludes normal writers;
repeated scans compare the observed bytes and records to catch direct concurrent changes. If a
bounded retry does not stabilize, lookup
refuses with `changed during lookup`. Malformed, missing, symlinked, or contradictory records and
store objects are refusals, not silently omitted rows. Existing POSIX `sha256:<hex>` filenames and
portable `sha256-<hex>` filenames share one logical identity; conflicting aliases refuse.

Lookup does not establish that the policy was correct, applied by a runtime, or used for a
particular decision. A digest does not authenticate a producer. Local retained history is not an
append-only or complete external audit log, and lookup is not a hosted policy control plane.

---

## Compatibility

- The legacy top-level `assay generate ...` and `assay record ...` paths were
  removed; use `assay policy generate ...` and `assay policy record ...`.
- Output shapes, exit codes, generated policy behavior, and policy schema
  semantics are unchanged.
