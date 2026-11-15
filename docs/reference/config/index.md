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
version: 1
suite: my-agent-tests
model: trace

tests:
  - id: args_valid
    input:
      prompt: "Look up customer 42"
    expected:
      type: args_valid
      policy: policies/default.yaml
```

Report formats are chosen on the command line, not in the config.

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
