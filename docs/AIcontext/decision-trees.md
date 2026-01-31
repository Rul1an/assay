# Decision Trees

> **Purpose**: Help AI agents decide which command, approach, or pattern to use.
> **Version**: 2.12.0 (January 2026)

## Decision Tree 1: Which Command Should I Use?

```mermaid
flowchart TD
    START[What do you want to do?] --> VALIDATE{Validate<br/>agent behavior?}
    VALIDATE -->|Yes| HAS_TRACES{Have traces?}
    VALIDATE -->|No| SETUP{Setup or<br/>configuration?}

    HAS_TRACES -->|No| CAPTURE[Capture traces first]
    CAPTURE --> SDK[Use Python SDK]
    CAPTURE --> IMPORT[assay import]

    HAS_TRACES -->|Yes| CI{In CI/CD?}
    CI -->|Yes| CMD_CI[assay ci]
    CI -->|No| CMD_RUN[assay run]

    SETUP -->|New project| INIT[assay init]
    SETUP -->|Add CI| INIT_CI[assay init --ci]
    SETUP -->|Debug| DOCTOR[assay doctor]

    START --> RUNTIME{Runtime<br/>enforcement?}
    RUNTIME -->|Yes| MCP[assay mcp wrap]
    RUNTIME -->|Kernel level| MONITOR[assay monitor]

    START --> EVIDENCE{Evidence/<br/>compliance?}
    EVIDENCE -->|Export| EV_EXPORT[assay evidence export]
    EVIDENCE -->|Verify| EV_VERIFY[assay evidence verify]
    EVIDENCE -->|Lint| EV_LINT[assay evidence lint]
```

## Decision Tree 2: Exit Code Handling

```mermaid
flowchart TD
    EXIT[Exit code?] --> E0{Code 0?}
    E0 -->|Yes| SUCCESS[✅ Success - continue]

    E0 -->|No| E1{Code 1?}
    E1 -->|Yes| TEST_FAIL[Test/Policy failure]
    TEST_FAIL --> EXPLAIN[Run: assay explain]
    EXPLAIN --> FIX_AGENT[Fix agent or relax policy]

    E1 -->|No| E2{Code 2?}
    E2 -->|Yes| CONFIG[Config error]
    CONFIG --> DOCTOR[Run: assay doctor]
    DOCTOR --> CHECK_FILES[Check file paths and syntax]

    E2 -->|No| E3{Code 3?}
    E3 -->|Yes| INFRA[Infrastructure error]
    INFRA --> RETRY[Retry with backoff]
    RETRY --> CHECK_API[Check API keys and limits]
```

## Decision Tree 3: Trace Recording Method

```mermaid
flowchart TD
    START[How to record traces?] --> LANG{Language?}

    LANG -->|Python| PY_SDK[Use AssayClient]
    PY_SDK --> PYTEST{Using pytest?}
    PYTEST -->|Yes| PLUGIN[Use pytest plugin]
    PYTEST -->|No| MANUAL[Manual client.record_trace]

    LANG -->|Other| IMPORT_METHOD{Have logs?}
    IMPORT_METHOD -->|MCP Inspector| MCP_IMPORT[assay import --format mcp-inspector]
    IMPORT_METHOD -->|JSONL| DIRECT[Direct JSONL file]
    IMPORT_METHOD -->|OTel| OTEL_IMPORT[assay import --format otel]

    LANG -->|CLI wrapper| CLI_TRACE[assay trace -- command]
```

## Decision Tree 4: Policy Development

```mermaid
flowchart TD
    START[Developing policy?] --> EXISTING{Have existing<br/>behavior to model?}

    EXISTING -->|Yes| LEARNING[Learning mode]
    LEARNING --> RECORD[assay record --capture]
    RECORD --> GENERATE[assay generate --from-profile]
    GENERATE --> REVIEW[Review and refine]

    EXISTING -->|No| MANUAL[Manual policy writing]
    MANUAL --> TEMPLATE[Start from example]
    TEMPLATE --> VALIDATE[assay validate --trace-file test.jsonl]
    VALIDATE --> ITERATE[Iterate on policy]

    REVIEW --> VALIDATE
    ITERATE --> COVERAGE[assay coverage]
    COVERAGE --> DONE{Coverage OK?}
    DONE -->|No| ITERATE
    DONE -->|Yes| DEPLOY[Deploy to CI]
```

## Decision Tree 5: CI Integration Method

```mermaid
flowchart TD
    START[Add CI?] --> METHOD{Preferred method?}

    METHOD -->|GitHub Action| ACTION["Rul1an/assay/assay-action@v2"]
    ACTION --> FEATURES{Need baseline?}
    FEATURES -->|Yes| BASELINE[Add baseline_key input]
    FEATURES -->|No| SIMPLE[Basic action config]

    METHOD -->|CLI only| CLI[assay ci command]
    CLI --> OUTPUTS{Need reports?}
    OUTPUTS -->|Yes| SARIF[--sarif + --junit flags]
    OUTPUTS -->|No| CONSOLE[Console output only]

    SARIF --> UPLOAD[Upload SARIF to GitHub]
```

## Decision Tree 6: Debugging Failures

```mermaid
flowchart TD
    START[Assay failing?] --> TYPE{Error type?}

    TYPE -->|Config error| CONFIG_DEBUG[Configuration issue]
    CONFIG_DEBUG --> DOCTOR[assay doctor]
    DOCTOR --> SYNTAX{Syntax OK?}
    SYNTAX -->|No| FIX_YAML[Fix YAML syntax]
    SYNTAX -->|Yes| PATHS[Check file paths]

    TYPE -->|Test failure| TEST_DEBUG[Test failure]
    TEST_DEBUG --> EXPLAIN[assay explain]
    EXPLAIN --> VIOLATION{Violation type?}
    VIOLATION -->|Policy| POLICY_FIX[Adjust policy or fix agent]
    VIOLATION -->|Metric| METRIC_FIX[Adjust threshold or fix output]

    TYPE -->|Infra error| INFRA_DEBUG[Infrastructure issue]
    INFRA_DEBUG --> API{API issue?}
    API -->|Yes| CHECK_KEY[Check API key and limits]
    API -->|No| NETWORK[Check network/timeout]
```

## Decision Tree 7: Runtime Security

```mermaid
flowchart TD
    START[Runtime enforcement?] --> LEVEL{Enforcement level?}

    LEVEL -->|Userspace| MCP[MCP Proxy]
    MCP --> DRY{Dry run first?}
    DRY -->|Yes| DRY_RUN[assay mcp wrap --dry-run]
    DRY -->|No| ENFORCE[assay mcp wrap --policy]

    LEVEL -->|Kernel| KERNEL[Kernel enforcement]
    KERNEL --> LINUX{Linux?}
    LINUX -->|Yes| MONITOR[assay monitor --pid]
    LINUX -->|No| FALLBACK[Use MCP proxy instead]

    LEVEL -->|Both| COMBINED[Tier 1 + Tier 2]
    COMBINED --> COMPILE[assay-policy compiles to both tiers]
```

## Decision Table: Command Selection

| Scenario | Command | Key Options |
|----------|---------|-------------|
| New project setup | `assay init` | `--ci` for CI workflow |
| Validate traces | `assay validate` | `--trace-file`, `--format sarif` |
| Run test suite | `assay run` | `--config`, `--baseline` |
| CI gate (strict) | `assay ci` | `--trace-file`, `--sarif` |
| Debug setup | `assay doctor` | (no options needed) |
| Explain failures | `assay explain` | `--trace-file` |
| Generate policy | `assay generate` | `--from-profile` |
| Record behavior | `assay record` | `--capture` |
| Check coverage | `assay coverage` | `--min-coverage 80` |
| MCP enforcement | `assay mcp wrap` | `--policy`, `--dry-run` |
| Export evidence | `assay evidence export` | `--profile`, `--out` |
| Verify evidence | `assay evidence verify` | `<bundle.tar.gz>` |
| Lint evidence | `assay evidence lint` | `--format sarif` |
| Sign tool | `assay tool sign` | `--key`, `--out` |

## Decision Table: Output Format Selection

| Use Case | Format | Flag |
|----------|--------|------|
| Human reading | Console | (default) |
| CI parsing | JSON | `--format json` |
| Test reporting | JUnit | `--format junit` or `--junit <path>` |
| GitHub Security | SARIF | `--format sarif` or `--sarif <path>` |

## Decision Table: Error Recovery

| Error | Reason Code | Recovery Action |
|-------|-------------|-----------------|
| Trace file not found | `E_TRACE_NOT_FOUND` | Check path, use `assay import` |
| Config parse error | `E_CFG_PARSE` | Run `assay doctor --config <file>` |
| Judge unavailable | `E_JUDGE_UNAVAILABLE` | Check API key, retry later |
| Rate limited | `E_RATE_LIMIT` | Wait, reduce parallelism |
| Test failed | `E_TEST_FAILED` | Run `assay explain` |
| Policy violation | `E_POLICY_VIOLATION` | Review policy or fix agent |

## When to Use What: Quick Reference

### For Validation
- **Quick check**: `assay validate --trace-file traces.jsonl`
- **Full suite**: `assay run --config assay.yaml`
- **CI gate**: `assay ci --config assay.yaml --strict`

### For Policy Development
- **From scratch**: Write `policy.yaml` manually
- **From behavior**: `assay generate --from-profile profile.json`
- **Test coverage**: `assay coverage --trace-file traces.jsonl`

### For Debugging
- **Setup issues**: `assay doctor`
- **Test failures**: `assay explain --trace-file traces.jsonl`
- **Coverage gaps**: `assay coverage`

### For CI Integration
- **GitHub Action** (recommended): `Rul1an/assay/assay-action@v2`
- **CLI only**: `assay ci` with `--sarif` flag
- **Custom**: `assay run` with manual report handling

### For Runtime Security
- **Userspace**: `assay mcp wrap --policy policy.yaml`
- **Kernel (Linux)**: `assay monitor --policy policy.yaml --pid <pid>`
- **Sandbox**: `assay sandbox --policy policy.yaml -- command`

## Related Documentation

- [Quick Reference](quick-reference.md) - Command cheat sheet
- [Entry Points](entry-points.md) - Full command documentation
- [User Flows](user-flows.md) - Complete user journeys
