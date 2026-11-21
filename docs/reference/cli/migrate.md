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
- A `sequence_valid` test's `policy:` file becomes its `rules:` (or `sequence:`). An empty
  `aliases: {}` can be omitted. Migration refuses a nonempty alias map or a field that the
  test would ignore, such as `tools.deny`.
- An untagged `expected:` mapping or a single-entry list becomes one tagged mapping, including in
  configs already marked version 1.
- A config with no version gets `configVersion: 1`.

When a referenced expected block names an argument or sequence policy, migrate reads the policy
relative to that block and inlines it in the same run. A JSON `schema_file` stays external.
Its relative path in the migrated config starts at the config directory. If path normalization
would select a different schema through a symlink, migration stops before writing.

The config is read strictly: a key the current version does not know refuses the config (exit 2),
and the file is left unchanged. The migrated YAML must fit the same 1 MiB limit as the input, including for `--dry-run`.
If it exceeds that limit, migration stops before creating the backup or writing the config.

For an `args_valid` policy, migration combines root `allow` and `deny` lists with their
counterparts under `tools`, in evaluation order (root entries first). This applies both to
inline policies and to policy files being inlined.

### Migrating a sequence rule that names a group

An external sequence policy can define an alias such as `Search: [A, B]`. Inline
`sequence_valid` rules have no group selector. For example, `{type: max_calls, tool: Search, max: 1}`
allows at most one call across both tools. A separate limit of one for each
tool also allows the trace `A, B`, which exceeds the shared limit.

If you need the shared limit, keep the original policy and verify that your
policy reader blocks a trace containing one call to each tool. Migration stops
before rewriting the eval config. Remove an alias
map only after checking that every selector already names the intended literal tool.

### Migrating an argument policy used with older traces

For a referenced `args_valid` policy with a JSON Schema per tool under `tools.arg_constraints`,
migration places those schemas in the inline policy and sets `enforcement.unconstrained_tools`
to `deny`. This preserves the older validator's refusal of a call to an unlisted tool. If the
policy also has a tool allowlist or denylist, review it manually. The 6.22.3 validator ignored
those lists when checking the schemas.

If you used `assay validate --trace-file` in 6.22.3, check how the calls were recorded. With an
object schema that accepts empty arguments, these cases gave different results on Linux.
Both 6.27.0 and the migration build read the same migrated config and trace in the final column.

| Recorded call in `meta.tool_calls` | 6.22.3 with the legacy policy | With the migrated policy |
|---|---|---|
| A listed tool with `args: {}` | Pass | Pass |
| The same call with the `args` key absent | Fail | Pass |
| The same call with `tool` in place of `tool_name` | Fail | Pass |
| A call object written directly instead of a list containing it | Pass | Fail |

Migration rewrites the config. It leaves the trace bytes as recorded.

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

The original is kept next to the config, as `eval.yaml.bak`. If migration needs
to write and that path already exists, it stops before changing the config.
If the existing file is a previous backup, move it to a safe location before
retrying. If the config references it, update the reference when moving it.
On Unix, a new backup is readable only by its owner by default.

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
      version: "2.0"
      schemas:
        deploy_service:
          type: object
          required:
          - port
```

### Policy Files

For an `args_valid` test, migrate replaces the `policy:` file reference with the file's full
value under `schema:`. For a map keyed by tool name, migration adds `version: "2.0"` and places
the map under `schemas:`. The same conversion applies to inline maps. Calls to unlisted tools
continue to pass this assertion. A structured policy keeps its tool
restrictions, argument schemas and enforcement settings. Migrate validates the resulting
assertion before writing the config. A `sequence_valid` test that points `policy:` at a rules file gets the rules inline as
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
