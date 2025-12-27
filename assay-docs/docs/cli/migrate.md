# assay migrate

Upgrade configuration from v0 to v1 format.

---

## Synopsis

```bash
assay migrate --config <CONFIG_FILE> [OPTIONS]
```

---

## Description

Migrates older Assay configuration files to the current v1 format. This includes:

- Converting sequence arrays to rule-based DSL
- Inlining external policy references
- Updating deprecated field names
- Adding required version field

---

## Options

| Option | Description |
|--------|-------------|
| `--config`, `-c` | Path to config file to migrate |
| `--dry-run` | Preview changes without writing |
| `--backup` | Create backup before modifying (default: true) |
| `--no-backup` | Skip backup creation |
| `--output`, `-o` | Write to different file instead of in-place |

---

## Examples

### Basic Migration

```bash
assay migrate --config eval.yaml

# Output:
# Migrating eval.yaml from v0 to v1...
# 
# Changes:
#   - Added: version: "1"
#   - Converted: sequences → rules DSL
#   - Inlined: policies/discount.yaml
#   - Renamed: threshold → min_score (deprecated)
# 
# Created backup: eval.yaml.bak
# Written: eval.yaml
```

### Preview Changes

```bash
assay migrate --config eval.yaml --dry-run

# Output:
# [DRY RUN] Would apply the following changes:
#
# --- eval.yaml (before)
# +++ eval.yaml (after)
# @@ -1,3 +1,4 @@
# +version: "1"
#  suite: my-agent
#  tests:
# ...
```

### Write to New File

```bash
assay migrate --config old-eval.yaml --output new-eval.yaml
```

---

## What Gets Migrated

### Version Field

```yaml
# Before (v0)
suite: my-agent

# After (v1)
version: "1"
suite: my-agent
```

### Sequence Rules

```yaml
# Before (v0)
tests:
  - id: order_check
    metric: sequence_valid
    sequences:
      - [get_customer, update_customer]
      - [verify_identity, delete_customer]

# After (v1)
tests:
  - id: order_check
    metric: sequence_valid
    rules:
      - type: before
        first: get_customer
        then: update_customer
      - type: before
        first: verify_identity
        then: delete_customer
```

### Inline Policies

```yaml
# Before (v0)
tests:
  - id: args_check
    metric: args_valid
    policy: policies/customer.yaml  # External file

# After (v1)
tests:
  - id: args_check
    metric: args_valid
    policy:  # Inlined
      tools:
        get_customer:
          arguments:
            id:
              type: string
              required: true
```

### Deprecated Fields

| v0 Field | v1 Field |
|----------|----------|
| `threshold` | `min_score` |
| `must_call` | `rules: [{ type: require }]` |
| `must_not_call` | `rules: [{ type: blocklist }]` |

---

## Backup Behavior

By default, migration creates a backup:

```
eval.yaml      → eval.yaml (updated)
eval.yaml.bak  → eval.yaml.bak (original)
```

Skip backup:

```bash
assay migrate --config eval.yaml --no-backup
```

---

## Migration Warnings

### Lossy Conversion

```
Warning: Lossy conversion detected

  The v0 field 'fuzzy_match' has no v1 equivalent.
  This field will be removed.
  
  If you rely on this behavior, consider:
    1. Using a custom metric
    2. Opening an issue for feature request
```

### Ambiguous Sequences

```
Warning: Ambiguous sequence conversion

  The sequence [A, B, C] could mean:
    - A before B, B before C (chain)
    - A before B, A before C (fan-out)
  
  Assuming chain behavior. Review the generated rules.
```

---

## Validation After Migration

After migrating, validate the new config:

```bash
assay validate --config eval.yaml

# Output:
# ✅ Config valid
# Version: 1
# Tests: 5
# Policies: 2 (inlined)
```

---

## Rollback

If migration causes issues, restore from backup:

```bash
mv eval.yaml.bak eval.yaml
```

---

## See Also

- [Configuration](../config/index.md)
- [Sequence Rules DSL](../config/sequences.md)
- [Migration Guide](../config/migration.md)
