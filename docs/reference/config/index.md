# Configuration

Learn how to configure Assay for your project.

---

## Configuration Files

| File | Purpose |
|------|---------|
| `eval.yaml` | Main test suite configuration |
| `assay.yaml` | Default MCP policy file for `assay mcp wrap` |
| `policy.yaml` | Alternate MCP policy filename; load it with `assay mcp wrap --policy policy.yaml` |

---

## Quick Reference

### Minimal Config

```yaml
# eval.yaml
version: "1"
suite: my-agent-tests

tests:
  - id: args_valid
    metric: args_valid
    policy: policies/default.yaml

output:
  format: [sarif, junit]
  directory: .assay/reports
```

---

## Sections

- [eval.yaml Reference](eval-yaml.md) — Full config options
- [Policy Files](policies.md) — MCP policy YAML and JSON Schema reference
- [Sequence Rules DSL](sequences.md) — Order constraints
- [Migration Guide](migration.md) — Upgrading from v0

---

## See Also

- [Quick Start](../../getting-started/quickstart.md)
- [CLI Reference](../cli/index.md)
