# Troubleshooting

Common errors and how to fix them.

---

## Configuration Errors (Exit Code 2)

### Missing configVersion

```
fatal: ConfigError: missing required field 'configVersion'
```

**Fix:** Add `configVersion: 1` at the top of your config:

```yaml
configVersion: 1  # Add this line
suite: my_suite
tests:
  # ...
```

### YAML Parse Error

```
fatal: ConfigError: failed to parse YAML: did not find expected node content at line 14 column 1, while parsing a flow node
```

**Common causes:**

1. **Missing colon after key:**
   ```yaml
   # Wrong
   type args_valid

   # Correct
   type: args_valid
   ```

2. **Incorrect indentation:**
   ```yaml
   # Wrong (mixed tabs/spaces)
   tests:
   	- id: test1  # Tab character

   # Correct (2 spaces)
   tests:
     - id: test1
   ```

3. **Unquoted special characters:**
   ```yaml
   # Wrong
   pattern: [a-z]+

   # Correct
   pattern: "[a-z]+"
   ```

**Debug tip:** Use a YAML validator like [yamllint](https://www.yamllint.com/) to find syntax errors.

### Unknown Policy Type

```
fatal: ConfigError: unknown policy type 'custom_check' in test 'my_test'
```

**Fix:** Use one of the supported policy types:

- `args_valid`
- `sequence_valid`
- `tool_blocklist`
- `regex_match`

### Duplicate Test ID

```
fatal: ConfigError: duplicate test id 'my_test'
```

**Fix:** Ensure all test IDs are unique within the suite.

---

## Test Failures (Exit Code 1)

### Missing Required Tool

```
❌ test_flow        failed: sequence_valid  (0.0s)
      Message: Missing required tool: notify_slack
```

**What it means:** Your config requires `notify_slack` to be called, but the trace doesn't contain that tool call.

**Possible fixes:**

1. **Update the trace:** Record a new trace that includes the tool call
2. **Remove the requirement:** If the tool is optional, remove the `require` rule
3. **Check tool name spelling:** Ensure the tool name matches exactly

### Blocked Tool Called

```
❌ security_test        failed: tool_blocklist  (0.0s)
      Message: Blocked tool called: delete_users
```

**What it means:** The agent called a tool that's on your blocklist.

**Possible fixes:**

1. **Fix the agent:** The agent shouldn't call this tool
2. **Update blocklist:** If the tool is now allowed, remove it from `blocked`

### Sequence Violation

```
❌ migration_flow        failed: sequence_valid  (0.0s)
      Message: Order violation: run_migration called before create_backup
```

**What it means:** Tools were called in the wrong order.

**Possible fixes:**

1. **Fix the agent logic:** Ensure tools are called in the correct order
2. **Update the rule:** If the order doesn't matter, remove the `before` rule

### Schema Validation Failed

```
❌ deploy_test        failed: args_valid  (0.0s)
      Message: Argument validation failed for deploy_service:
        - port: expected integer, got string "8080"
```

**What it means:** The tool was called with arguments that don't match the schema.

**Possible fixes:**

1. **Fix the agent:** Ensure arguments have correct types
2. **Loosen the schema:** If string is acceptable, update the schema

### Regex Not Matched

```
❌ output_test        failed: regex_match  (0.0s)
      Message: Output did not match pattern: "temperature is \d+ degrees"
```

**What it means:** The agent's output doesn't match the expected pattern.

**Debug tip:** Check the actual output in the trace file to see what was returned.

---

## Trace Issues

### Trace File Not Found

```
fatal: IOError: trace file not found: traces/golden.jsonl
```

**Fix:** Check the path and ensure the file exists:

```bash
ls -la traces/
```

### Invalid Trace Format

```
fatal: TraceError: invalid JSON at line 42: expected ',' or '}'
```

**Fix:** Validate the JSONL file:

```bash
# Check for JSON errors
cat trace.jsonl | jq -c . > /dev/null
```

### Empty Trace

```
fatal: TraceError: trace file is empty: traces/empty.jsonl
```

**Fix:** Ensure your recording captured events. Re-record if necessary.

---

## Cache Issues

### Unexpected Skips

```
Running 5 tests...
⏭️  test_1        skipped (fingerprint match)
⏭️  test_2        skipped (fingerprint match)
⏭️  test_3        skipped (fingerprint match)
```

**What it means:** Tests are being skipped because the trace fingerprint matches a previous run.

**To force re-run:**

```bash
# Option 1: Use fresh database
assay run --config eval.yaml --trace-file trace.jsonl --db :memory:

# Option 2: Delete the cache
rm -rf .assay/store.db
```

### Cache Corruption

```
fatal: CacheError: failed to read cache: database disk image is malformed
```

**Fix:** Delete and rebuild the cache:

```bash
rm -rf .assay/
assay run --config eval.yaml --trace-file trace.jsonl
```

---

## Migration Issues

### External Policy Not Found

```
fatal: MigrationError: could not read policy file: policies/args.yaml
```

**Fix:** Ensure the policy file exists at the referenced path.

### Already Migrated

```
warn: Config already has configVersion: 1, skipping migration
```

**What it means:** The config is already in v1 format. No action needed.

---

## Python / SDK Issues

### `pip install assay` vs `assay-it`

If you ran `pip install assay`, you installed an unrelated package.

**Fix:**
```bash
pip uninstall assay
pip install assay-it
```

### Module Not Found
```
ModuleNotFoundError: No module named 'assay'
```

**Fix:** Ensure you have installed the package (it exposes the `assay` module):
```bash
pip install assay-it
```

### Trace Recording Empty
If your trace file is created but has no events:

1.  Ensure you call `writer.write_trace()` or use the context manager.
2.  Check if `record_chat_completions_with_tools` actually ran.

---

## CI/CD Issues

### Non-Zero Exit in CI

```
Error: Process completed with exit code 1.
```

**Meaning:** One or more tests failed. Check the logs for specific failures.

**Common CI fixes:**

1. **Ensure trace files are committed:**
   ```yaml
   - uses: actions/checkout@v4
     with:
       lfs: true  # If using Git LFS for traces
   ```

2. **Use correct paths:**
   ```yaml
   - run: assay run --config ./path/to/eval.yaml --trace-file ./path/to/trace.jsonl
   ```

3. **Install Assay in CI:**
   ```yaml
   - name: Install Assay
     run: cargo install assay-cli
   ```

### Permission Denied

```
fatal: IOError: permission denied: .assay/store.db
```

**Fix:** Ensure the runner has write permissions, or use in-memory mode:

```bash
assay run --config eval.yaml --trace-file trace.jsonl --db :memory:
```

---

## Getting Help

If you're stuck:

1. **Enable debug logging:**
   ```bash
   RUST_LOG=assay=debug assay run --config eval.yaml --trace-file trace.jsonl
   ```

2. **Check the GitHub Issues:**
   [github.com/Rul1an/assay/issues](https://github.com/Rul1an/assay/issues)

3. **File a bug report** with:
   - Assay version (`assay --version`)
   - Full error output
   - Minimal config to reproduce
