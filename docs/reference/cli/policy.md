# assay policy

Policy authoring, validation, formatting, and migration commands.

The policy family now owns policy-authoring commands. The legacy top-level
forms `assay generate` and `assay record` remain available as hidden
compatibility shims and print deprecation warnings.

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

---

## Compatibility

- `assay generate ...` still runs as a hidden compatibility shim for
  `assay policy generate ...`.
- `assay record ...` still runs as a hidden compatibility shim for
  `assay policy record ...`.
- Output shapes, exit codes, generated policy behavior, and policy schema
  semantics are unchanged.
