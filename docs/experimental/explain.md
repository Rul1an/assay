# Assay Explain

The `assay explain` command visualizes the evaluation of a trace against a policy. It provides a step-by-step breakdown of how each rule was applied, explaining why tool calls were allowed or blocked.

## Usage

```bash
assay explain [OPTIONS] --policy <POLICY> --trace <TRACE>
```

## Arguments

| Argument | Description | Required |
|----------|-------------|----------|
| `-p, --policy <POLICY>` | Path to the policy file (`.yaml`) | Yes |
| `-t, --trace <TRACE>` | Path to the trace file (JSON, JSONL, or OTel) | Yes |
| `-f, --format <FORMAT>` | Output format: `terminal`, `markdown`, `html`, `json` | No (default: `terminal`) |
| `-o, --output <FILE>` | Output file path (defaults to stdout) | No |
| `--verbose` | Show passed rules in terminal output | No |
| `--blocked-only` | Show only blocked steps and failures | No |

## Examples

### Terminal Output

Evaluate a trace and see the result in your terminal with colored status indicators:

```bash
assay explain -p policy.yaml -t trace.json
```

Output:
```text
Policy: Banking Policy (v1.1)
Trace: 5 steps (4 allowed, 1 blocked)

Timeline:
  [0] Login()                                  ✅ allowed
  [1] CheckBalance(account: "123")             ✅ allowed
  [2] Transfer(amount: 1000)                   ✅ allowed
  [3] Logout()                                 ✅ allowed
  [4] DeleteAccount(id: "123")                 ❌ BLOCKED
      └── Rule: deny_list
      └── Reason: Tool 'DeleteAccount' is in deny list
```

### Markdown Report

Generate a Markdown report for CI/CD summaries or documentation:

```bash
assay explain -p policy.yaml -t trace.json -f markdown -o report.md
```

### HTML Report

Create a self-contained HTML file for sharing with stakeholders:

```bash
assay explain -p policy.yaml -t trace.json -f html -o report.html
```

## Trace Formats

`assay explain` supports multiple trace input formats:

### 1. Simple JSON Array

A list of tool names (strings) or tool call objects:

```json
[
  "Login",
  "CheckBalance",
  "Logout"
]
```

Or with arguments:

```json
[
  { "name": "Login", "args": { "user": "alice" } },
  { "name": "Logout" }
]
```

### 2. Assay Trace Object

The standard trace format used by Assay:

```json
{
  "id": "trace-123",
  "tools": [
    { "name": "Login" },
    { "name": "Logout" }
  ]
}
```

### 3. JSONL (JSON Lines)

One tool call per line:

```json
{"name": "Login"}
{"name": "Logout"}
```

### 4. OpenTelemetry (OTel)

Supports standard OTel span JSON structure, extracting tool names from span names or attributes.

## Integration

This command is useful for:
- **Debugging**: Understanding why a specific trace failed in CI.
- **Auditing**: Generating human-readable reports of agent activity.
- **Policy Development**: Verifying that your policy rules behave as expected against sample traces.
