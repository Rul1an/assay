# Migration Guide

Assay follows semantic versioning. Configuration files use a `configVersion` field to ensure backward compatibility while allowing the schema to evolve.

---

## v0 to v1 Migration

**Assay v0.8.0** introduces `configVersion: 1`.
The primary change is the handling of **policies** and **strictness**.

### Key Changes
1.  **Policies are Inlined**: The top-level `policies` list (used in v0) is deprecated. Policies are now resolved and inlined into `test.expected.schema` or `test.expected.policy` during migration.
2.  **Strict Validation**: The `assay migrate` command now strictly enforces the schema. It will fail if it detects legacy fields like `policies` in a v1 config.

### How to Migrate

Run the `migrate` command on your configuration file:

```bash
assay migrate --config mcp-eval.yaml
```

This will:
1.  Read your v0 configuration.
2.  Resolve any external policy files referenced in `policies: [...]`.
3.  Inline them into the respective tests.
4.  Remove the top-level `policies` field.
5.  Set `configVersion: 1`.
6.  Back up the original file to `mcp-eval.yaml.bak`.

### Common Errors

If you try to run `assay migrate` on a file that has valid `configVersion: 1` but still contains legacy fields (e.g., if you manually edited it), you will see:

```text
fatal: failed to load config (strict check failed)
Caused by:
    ConfigError: Top-level 'policies' is not valid in configVersion: 1.
    Did you mean to run assay migrate on a v0 config, or remove legacy keys?
```

**Fix:** Remove the legacy fields manually or revert `configVersion` to `0` to force a re-migration.

### Rollback

If validation fails or migration causes issues:

```bash
# Option 1: Restore from backup (created by migrate command)
cp mcp-eval.yaml.bak mcp-eval.yaml

# Option 2: Revert Python SDK to previous stable version
pip install assay-it==0.8.0
```

---

## CI/CD Checks

In your Continuous Integration (CI) pipeline, you should ensure that all configuration files are fully migrated and up-to-date. Use the `--check` flag:

```bash
assay migrate --check --config mcp-eval.yaml
```

**Exit Codes:**
*   **0**: Clean. The config is up-to-date (v1) and requires no changes.
*   **2**: Dirty. The config is legacy (v0) or contains errors/unknown fields.

**Example CI Step:**

```yaml
- name: Verify Config Migration
  run: assay migrate --check --config mcp-eval.yaml
```
