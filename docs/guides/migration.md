# Migration Guide

Upgrading from legacy v0 configs to v1.

---

## Why Migrate?

Assay v1 configs are:

- **Versioned:** the config says which schema it uses (`configVersion: 1`)
- **Fewer files:** referenced expected blocks and policy files are inlined where `assay migrate` can
  inline them, so the config carries its own checks

---

## Automatic Migration

The `assay migrate` command handles most upgrades automatically:

```bash
assay migrate --config eval.yaml
```

### What It Does

1. **Creates backup:** `eval.yaml` → `eval.yaml.bak`
2. **Inlines references:** an `expected: {$ref: <file>}` becomes the referenced `expected:` block; an
   `args_valid` `policy:` file becomes `schema:`; a `sequence_valid` `policy:` file becomes `rules:`.
   A `policy:` inside a referenced expected file is kept as a reference: the migrated config still
   reads that file, so keep it.
3. **Adds version:** `configVersion: 1` header added

`assay migrate --check --config eval.yaml` exits 2 when the config still needs migrating and 0 when
it does not. Without `--config` it reads `mcp-eval.yaml`. A config with a key the current version
does not know is refused (exit 2) and left unchanged.

---

## Before / After Examples

### External References

**Before (v0):**
```yaml
# eval.yaml
suite: my_agent
model: gpt-4o-mini
tests:
  - id: deploy_args
    input:
      prompt: "Deploy the service on port 8080"
    expected:
      $ref: policies/args.yaml
  - id: deploy_notifies
    input:
      prompt: "Deploy the service on port 8080"
    expected:
      $ref: policies/sequence.yaml
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

**After (v1)**, as `assay migrate` writes it:
```yaml
configVersion: 1
suite: my_agent
model: gpt-4o-mini
tests:
- id: deploy_args
  input:
    prompt: Deploy the service on port 8080
  expected:
    type: args_valid
    schema:
      deploy_service:
        type: object
        required:
        - port
- id: deploy_notifies
  input:
    prompt: Deploy the service on port 8080
  expected:
    type: sequence_valid
    rules:
    - type: require
      tool: notify_slack
```

### Legacy Sequence Format

`assay migrate` does not convert an `expected:` block of `type: sequence` with a `tools:` list; a
config that has one is refused. Write the order as `sequence_valid` rules by hand:

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

<!-- docs-example: not-a-config (the expected block is elided) -->
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
