# assay explain

Explain how a trace is evaluated against policy rules, step by step.

---

## Synopsis

```bash
assay explain --trace <TRACE> --policy <POLICY> [OPTIONS]
```

---

## Description

`assay explain` loads a trace and policy, evaluates each tool call, and prints why steps were allowed or blocked.

Use it for:
- fast triage of blocked traces
- rule-level debugging (`rule_id`, `rule_type`, explanation context)
- compliance-oriented reporting with `--compliance-pack`

---

## Options

| Option | Description |
|--------|-------------|
| `--trace`, `-t <FILE>` | Trace input (JSON, JSONL, or object with `tools`/`tool_calls`) |
| `--policy`, `-p <FILE>` | Policy file used for evaluation |
| `--format`, `-f <FORMAT>` | Output format: `terminal` (default), `markdown`, `html`, `json` |
| `--output`, `-o <FILE>` | Write output to file instead of stdout |
| `--blocked-only` | Show only blocked steps (terminal output only) |
| `--verbose` | Show all rule evaluations per step (terminal output only) |
| `--compliance-pack <REF>` | Add article hints + coverage summary from a compliance pack (for `terminal`/`markdown`) |

---

## Compliance Pack Output

When `--compliance-pack` is provided:
- `terminal` and `markdown` outputs include:
  - **Compliance Coverage** (`<applicable>/<total>` + percentage)
  - **Blocking Rule Hints** (`rule_id -> article`)
- `json` and `html` outputs are unchanged in this slice.

Definition:
- **`total`** = number of rules in the loaded compliance pack.
- **`applicable`** = number of unique evaluated `rule_id`s in the trace explanation that resolve to an article reference (pack mapping first, native fallback mapping second).

---

## Examples

### Basic explain

```bash
assay explain --trace traces/session.jsonl --policy policy.yaml
```

### Blocked-only terminal report

```bash
assay explain \
  --trace traces/session.jsonl \
  --policy policy.yaml \
  --blocked-only \
  --format terminal
```

### Markdown report (full explanation)

```bash
assay explain \
  --trace traces/session.jsonl \
  --policy policy.yaml \
  --format markdown \
  --output reports/explain.md
```

### Explain with compliance hints

```bash
assay explain \
  --trace traces/session.jsonl \
  --policy policy.yaml \
  --compliance-pack eu-ai-act-baseline \
  --format terminal
```

Expected terminal tail:

```text
Compliance Coverage:
  eu-ai-act-baseline: 3/8 rules applicable (37.5%)

Compliance Hints:
  - deny_list -> Article 15(3) - Robustness and accuracy
```

Expected markdown tail:

```md
## Compliance Coverage
- eu-ai-act-baseline: 3/8 rules applicable (37.5%)

### Blocking Rule Hints
- `deny_list` -> Article 15(3) - Robustness and accuracy
```

---

## Exit Code

- `0` if all steps are allowed
- `1` if one or more steps are blocked
