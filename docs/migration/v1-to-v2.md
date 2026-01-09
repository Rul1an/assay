# Migrating from v1 to v2 Policies

Assay v2.0 introduces a new, standardized policy format based on [JSON Schema](https://json-schema.org/). This replaces the ad-hoc `constraints` syntax used in v1.x.

**Status:**
- **v1.6.0**: v2 format support introduced.
- **v1.7.0**: v1 format is deprecated. Startup warnings are emitted. Strict mode available.
- **v2.0.0**: v1 format support will be removed.

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

## Manual Migration Guide

If you prefer to migrate manually, here is how the syntax maps.

### v1.x (Legacy)

```yaml
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
version: "2.0-schema"
tools:
  deny: ["exec_command"]
  arg_constraints:
    read_file:
      path:
        type: string
        pattern: "^/data/.*"
```

Alternatively, you can provide a full schema for the tool arguments:

```yaml
version: "2.0-schema"
tools:
  arg_constraints:
    read_file:
      type: object
      properties:
        path:
          type: string
          pattern: "^/data/.*"
      required: ["path"]
      additionalProperties: false
```

## Strict Mode (CI/CD)

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
