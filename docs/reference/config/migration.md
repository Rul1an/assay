# Migration Guide

Assay follows semantic versioning. Configuration files use a `configVersion` field to ensure backward compatibility while allowing the schema to evolve.

---

## v0 to v1 Migration

**Assay v0.8.0** introduces `configVersion: 1`.
The primary change is the handling of **policies** and **strictness**.

### Key Changes
1.  **Referenced files are inlined**: `assay migrate` writes the files a test references into the
    test: an `expected: {$ref: <file>}`, an `args_valid` `policy:` file, a `sequence_valid`
    `policy:` file. A top-level `policies` list is not read by any version; a config with one is
    refused.
2.  **Strict Validation**: `assay migrate` reads the config strictly. A key the current version does
    not know refuses the config (exit 2), whatever its version, and the file is left unchanged.

### How to Migrate

Run the converter from the working directory you use for `assay run`. A relative
`args_valid.policy` or `sequence_valid.policy` in the eval file is read from
that directory first. For a file-only check, if that path has no entry there, the converter
also looks beside the config. An invalid file or dangling link in the working directory
stops conversion. Mixed sequence checks require the working-directory source. A
referenced expected block (`$ref`) is resolved relative to the eval file:

```bash
assay migrate --config eval.yaml
```

This will:
1.  Read your v0 configuration, refusing any key the current version does not know.
2.  Inline the files your tests reference: an `expected: {$ref: <file>}`, an `args_valid` `policy:`
    file, a `sequence_valid` `policy:` file.
3.  Set `configVersion: 1`.
4.  Back up the original file to `eval.yaml.bak`.

### Common Errors

A key that is not part of the config refuses it, whatever its version. A top-level `policies` list,
for example, is not a format `assay migrate` reads:

```text
fatal: failed to load config (strict check failed)

Caused by:
    ConfigError: failed to parse YAML: unknown field `policies`, expected one of `configVersion`, `version`, `suite`, `model`, `settings`, `thresholds`, `otel`, `tests` at line 4 column 1
```

**Fix:** Move each referenced file into the test that uses it, as `expected: {$ref: <file>}` or a
`policy:` on its `args_valid` or `sequence_valid` block, and run `assay migrate` again.

### Rollback

If validation fails or migration causes issues:

```bash
# Option 1: Restore from backup (created by migrate command)
cp eval.yaml.bak eval.yaml

# Option 2: Revert Python SDK to previous stable version
pip install assay-it==0.8.0
```

---

## CI/CD Checks

In your Continuous Integration (CI) pipeline, you should ensure that all configuration files are fully migrated and up-to-date. Use the `--check` flag:

```bash
assay migrate --check --config eval.yaml
```

**Exit Codes:**
*   **0**: Clean. The config is up-to-date (v1) and requires no changes.
*   **2**: Dirty. The config is legacy (v0) or contains errors/unknown fields.

**Example CI Step:**

```yaml
- name: Verify Config Migration
  run: assay migrate --check --config eval.yaml
```
