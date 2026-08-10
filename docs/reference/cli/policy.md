# assay policy

Policy authoring, validation, formatting, and migration commands.

The policy family owns policy-authoring commands. The legacy top-level forms
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

## Compatibility

- The legacy top-level `assay generate ...` and `assay record ...` paths were
  removed; use `assay policy generate ...` and `assay policy record ...`.
- Output shapes, exit codes, generated policy behavior, and policy schema
  semantics are unchanged.
