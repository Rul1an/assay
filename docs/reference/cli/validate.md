# assay validate

Validate agent traces against your policy. The standard CI gate.

---

## Synopsis

```bash
assay validate [OPTIONS]
```

## Description

`validate` is a specialized, lightweight command designed for CI/CD pipelines. It checks if the tool calls in your trace file adhere to the schema and sequence rules defined in `assay.yaml`.

Unlike `assay run`, which can perform active replay and LLM-as-a-Judge evaluation, `validate` is strictly **static analysis** of the trace. It is deterministic, fast (<10ms), and safe to run anywhere.

## Options

### Input
| Option | Description |
|--------|-------------|
| `--config <FILE>` | Path to config. Default: `assay.yaml`. |
| `--trace-file <FILE>` | Trace file to validate (JSONL). |
| `--baseline <FILE>` | Compare against a baseline trace. |

### Output
| Option | Description |
|--------|-------------|
| `--format <FMT>` | Output format: `text` (default), `json`, `sarif`. |
| `--output <FILE>` | Write report to file (e.g., `report.sarif`). |

## Exit Codes

Designed for CI pipelines:

| Code | Meaning | Action |
|------|---------|--------|
| `0` | **Pass**. No errors. | ✅ Proceed. |
| `1` | **Fail**. Policy violation. | ❌ Block PR. |
| `2` | **Error**. Config/Schema invalid. | ⚠️ Fix setup. |

## Agentic Output

Use `--format json` to get a structured, machine-parsable report that follows the **Assay Agentic Contract**. This allows AI agents to read the report and self-correct their policies.

```json
{
  "ok": false,
  "exit_code": 1,
  "diagnostics": [
    {
      "code": "E_SCHEMA_VIOLATION",
      "severity": "error",
      "message": "Value exceeds max (50 > 30)",
      "fix_steps": ["Adjust the agent prompt or relax the policy."]
    }
  ],
  "suggested_actions": [...]
}
```

## GitHub Advanced Security

Use `--format sarif` to integrate directly with GitHub Code Scanning.

```bash
assay validate --trace-file traces.jsonl --format sarif --output results.sarif
```
