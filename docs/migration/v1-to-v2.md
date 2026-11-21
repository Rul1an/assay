# Migrating from v1 to v2 Policies

The v2 policy format uses [JSON Schema](https://json-schema.org/) in place of the v1 `constraints` syntax.

**Status:**
- **v1.6.0**: v2 format support introduced.
- **v1.7.0**: v1 format is deprecated. Startup warnings are emitted. Strict mode available.
- **Assay 6.x** continues to read v1 policies. [ADR-052](../architecture/ADR-052-one-closed-reader-one-file-form.md) schedules their removal from normal readers for **Assay 7.0.0**. The converter will still read the old format.

## Improving Security & Standards

The v1 format used regex-based constraints which were simple but limited. They couldn't easily handle nested objects, arrays, or type validation.
The v2 format leverages full JSON Schema, allowing you to:
- Validate complex nested argument structures.
- Enforce types (string, integer, array).
- Use standard validation keywords (`minLength`, `pattern`, `enum`).
- Benefit from the vast ecosystem of JSON Schema tools.

## Migration Tool

Assay includes a built-in migration tool to automatically convert your existing policies.

```bash
# Preview changes (dry run)
assay policy migrate --input policy.yaml --dry-run

# Apply migration (overwrites input file)
assay policy migrate --input policy.yaml
```

On Assay v6.27.0, use `--dry-run` to preview. On that release, `--check` alone writes the result over the input.

For a policy used with `assay coverage`, check that `assay policy migrate --help` lists `--coverage`. Preview the change, then write to a new file:

```bash
assay policy migrate --coverage --input policy.yaml --dry-run
assay policy migrate --coverage --input policy.yaml --output migrated.yaml
```

Coverage treats names as text. Quote numeric-looking names if YAML reads two alias names as the same number (such as `"0x10"` and `"16"`), or if a name is too large to parse as a YAML integer. The input and the converted file must each fit the coverage reader's 1,000,000-byte limit.

## Manual Migration Guide

If you prefer to migrate manually, here is how the syntax maps.

### v1.x (Legacy)

```yaml
# Before (policy v1)
version: "1.0"
deny: ["exec_command"]
constraints:
  - tool: read_file
    params:
      path:
        matches: "^/data/.*"
```

### v2.0 (New)

```yaml
version: "2.0"
tools:
  deny: ["exec_command"]
schemas:
  read_file:
    type: object
    additionalProperties: true
    properties:
      path:
        type: string
        minLength: 1
        pattern: "^/data/.*"
    required: ["path"]
```

The v1 constraint requires a nonempty `path` and permits other arguments. The converted schema preserves both behaviours. Setting `additionalProperties` to `false` would also reject calls that supply extra arguments.

## Strict mode and the 7.0 cut

In 7.0, normal MCP policy readers require the current format. The explicit
migration command accepts v1 input even with `ASSAY_STRICT_DEPRECATIONS=1`.
For an activated v1 policy, migrate a copy, then use `assay policy activate` to
replace the active policy. Existing activation records retain their original
input and policy digests.

### Assay 6.x

To ensure no new v1 policies are introduced into your codebase, enabling strict mode in your CI pipeline is recommended.

**CLI Flag:**
```bash
assay policy validate --deny-deprecations --input policy.yaml
```

**Environment Variable:**
```bash
export ASSAY_STRICT_DEPRECATIONS=1
assay run ...
```

This will cause Assay to exit with an error if any legacy v1 policy constructs are detected.
