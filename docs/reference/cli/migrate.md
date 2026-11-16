# assay migrate

Upgrade an eval config from v0 to v1.

---

## Synopsis

```bash
assay migrate [--config <CONFIG_FILE>] [--dry-run] [--check]
```

---

## Description

`assay migrate` reads an eval config, inlines the files it references, and writes it back as a v1
config:

- An `expected: {$ref: <file>}` becomes the `expected:` block in that file.
- An `args_valid` test's `policy:` file becomes its `schema:`.
- A `sequence_valid` test's `policy:` file becomes its `rules:` (or `sequence:`). A file that
  declares `aliases` or another policy field cannot be written as inline rules and is refused.
- A config with no version gets `configVersion: 1`.

A `policy:` inside a referenced expected file is not inlined: the migrated test keeps
`policy: <file>`, and the config still reads that file at run time, so keep it.

The config is read strictly: a key the current version does not know refuses the config (exit 2),
and the file is left unchanged.

---

## Options

| Option | Description |
|--------|-------------|
| `--config <CONFIG_FILE>` | Config to migrate (default: `mcp-eval.yaml`) |
| `--dry-run` | Print the migrated config instead of writing it |
| `--check` | Exit 2 when the config needs migrating, 0 when it does not; change nothing |

---

## Examples

### Basic Migration

```bash
assay migrate --config eval.yaml

# Output:
# Migrating configuration: "eval.yaml"
# Loaded config version: 0 (model: gpt-4o-mini, suite: my_agent)
# Original config backed up to: "eval.yaml.bak"
# ✅ Migration complete! Updated "eval.yaml"
```

The original is kept next to the config, as `eval.yaml.bak`.

### Preview Changes

```bash
assay migrate --config eval.yaml --dry-run
```

prints the migrated config after `--- Migrated Config (Dry Run) ---` and writes nothing.

### Check in CI

```bash
assay migrate --config eval.yaml --check
```

exits 2 with `Migration required for "eval.yaml"` while the config still needs migrating. On a
config that does not, both `--check` and a plain run exit 0; the plain run prints
`No legacy policies found. Config is already up to date.`

---

## What Gets Migrated

### Referenced Expected Blocks

```yaml
# Before (v0)
suite: my_agent
model: gpt-4o-mini
tests:
  - id: deploy_args
    input:
      prompt: "Deploy the service on port 8080"
    expected:
      $ref: policies/args.yaml
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
# After (v1)
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
```

### Policy Files

An `args_valid` test that points `policy:` at a map from tool name to schema gets that map inline as
`schema:`. A policy file in the structured form (`version` and `schemas:`) is not inlined: migrate
stops with an error that names the file and says to keep the reference, with exit 2, also under
`--check`. A `sequence_valid` test that points `policy:` at a rules file gets the rules inline as
`rules:`. The [Sequence Rules DSL](../config/sequences.md) describes that file.

### What Is Not Converted

An `expected:` block of `type: sequence` with a `tools:` list is not converted; the config is
refused. Write the order as `sequence_valid` rules ([Migration Guide](../../guides/migration.md)).

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
- [Migration Guide](../../guides/migration.md)
