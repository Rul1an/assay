# Migration Guide

Upgrading from legacy v0 configs to v1.

---

## Why Migrate?

Assay v1 configs are:

- **Self-contained:** No external policy file references
- **Reproducible:** Everything in one file
- **Portable:** Copy-paste works
- **Versioned:** Clear schema versioning

---

## Automatic Migration

The `assay migrate` command handles most upgrades automatically:

```bash
assay migrate --config eval.yaml
```

### What It Does

1. **Creates backup:** `eval.yaml` → `eval.yaml.bak`
2. **Inlines policies:** External `$ref` files are merged into the main config
3. **Adds version:** `configVersion: 1` header added
4. **Updates syntax:** Legacy sequence format converted to DSL

---

## Before / After Examples

### External Policy References

**Before (v0):**
```yaml
# eval.yaml
suite: my_agent
tests:
  - id: deploy_test
    policies:
      - $ref: policies/args.yaml
      - $ref: policies/sequence.yaml
```

```yaml
# policies/args.yaml
type: args_valid
schema:
  deploy_service:
    type: object
    required: [port]
```

```yaml
# policies/sequence.yaml
type: sequence_valid
rules:
  - type: require
    tool: notify_slack
```

**After (v1):**
```yaml
# eval.yaml (migrated)
configVersion: 1
suite: my_agent
tests:
  - id: deploy_test
    input:
      prompt: ""  # May need to be filled in
    expected:
      type: args_valid
      schema:
        deploy_service:
          type: object
          required: [port]
```

### Legacy Sequence Format

**Before (v0):**
```yaml
expected:
  type: sequence
  tools:
    - tool_a
    - tool_b
    - tool_c
```

**After (v1):**
```yaml
expected:
  type: sequence_valid
  rules:
    - type: before
      first: tool_a
      then: tool_b
    - type: before
      first: tool_b
      then: tool_c
```

---

## Manual Steps After Migration

The migration tool handles syntax, but you may need to:

### 1. Add Input Prompts

Migration can't infer prompts. Add them manually:

```yaml
tests:
  - id: deploy_test
    input:
      prompt: "Deploy to staging"  # Add this
    expected:
      # ...
```

### 2. Review Inlined Schemas

Check that merged schemas are correct:

```yaml
expected:
  type: args_valid
  schema:
    deploy_service:
      # Verify this matches your tool's actual schema
      type: object
      required: [port, env]
```

### 3. Delete External Files

After verifying migration, remove old policy files:

```bash
rm -rf policies/
```

---

## Verification

After migration, verify the config works:

```bash
# 1. Check syntax
assay run --config eval.yaml --trace-file trace.jsonl --db :memory:

# 2. Compare results with old config (if you kept a backup)
# Results should be identical
```

---

## Rollback

If something goes wrong:

```bash
# Restore from backup
cp eval.yaml.bak eval.yaml
```

---

## Breaking Changes in v1

| v0 Feature | v1 Equivalent |
|------------|---------------|
| `policies: [$ref: ...]` | `expected: { type: ..., ... }` (inline) |
| `type: sequence` (list) | `type: sequence_valid` (rules DSL) |
| No version field | `configVersion: 1` required |

---

## Getting Help

If automatic migration fails:

1. Check the error message for specific issues
2. Manually copy policy content into the main config
3. Add `configVersion: 1` at the top
4. Run `assay run` to validate

For complex migrations, [open an issue](https://github.com/Rul1an/assay/issues).
