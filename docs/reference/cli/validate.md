# assay validate

Validate agent traces against your policy. The standard CI gate.

---

## Synopsis

```bash
assay validate [OPTIONS] [CONFIG]
```

## Description

`validate` is a specialized, lightweight command designed for CI/CD pipelines. With `--trace-file`, it checks the tool calls in your trace file against the schema and sequence rules defined in `assay.yaml`.

A `sequence_valid` or `args_valid` test gets the verdict `assay run` gives it on the same trace, from the same evaluator. A test whose rules the trace breaks is reported as `E_SEQUENCE_VIOLATION`, and one whose argument policy it breaks as `E_ARGS_VIOLATION`, with a record of each refused call in the diagnostic's context; both exit `1`. An `args_valid` test reads its inline argument policy from `expected.schema`. For example, put `version: "2.0"` and `schemas` there, and place each tool's JSON Schema under its name in `schemas`.

A config that refers to an external expected policy or uses a tool-name map directly under `args_valid.schema` is refused during loading (exit `2`). Run `assay migrate --config eval.yaml` from the working directory used for the old test, then validate the migrated config.

Apart from the schema and sequence rules above, `validate` evaluates no `expected:` kind and no `assertions:`. Use `assay run` for those.

Unlike `assay run`, which can perform active replay and LLM-as-a-Judge evaluation, `validate` is strictly **static analysis** of the trace. It is deterministic, fast (<10ms), and safe to run anywhere.

## Options

### Input
| Option | Description |
|--------|-------------|
| `[CONFIG]` | Optional positional config path. |
| `--config <FILE>` | Path to config. Default: `assay.yaml`. Mutually exclusive with `[CONFIG]`. |
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

Both successful and failing JSON reports use `schema: "assay.validate_report.v1"` with integer `schema_version: 1`. Consumers should branch on both fields rather than treating every version `1` document as the same shape.

```json
{
  "schema": "assay.validate_report.v1",
  "schema_version": 1,
  "ok": false,
  "exit_code": 1,
  "diagnostics": [
    {
      "code": "E_ARGS_VIOLATION",
      "severity": "error",
      "message": "Test 'search': args_valid failed: 1 errors",
      "fix_steps": ["Run `assay run` with this trace for the per-call record"]
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

Equivalent config path forms:

```bash
assay validate eval.yaml --trace-file traces.jsonl
assay validate --config eval.yaml --trace-file traces.jsonl
```
