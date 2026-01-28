# Assay GitHub Action

**Catch AI agent security issues before they hit production.**

## What It Does

```
Your AI Agent → Generates Traces → Assay Verifies → PR Feedback
```

| Step | What Happens |
|------|--------------|
| 1. **Record** | Your tests generate evidence bundles (what your agent did) |
| 2. **Verify** | Assay checks bundle integrity (no tampering) |
| 3. **Lint** | Assay scans for security issues (file access, network calls, shell commands) |
| 4. **Report** | Results appear in GitHub Security tab + PR comments |

## Why Use It

| Problem | Without Assay | With Assay |
|---------|---------------|------------|
| Agent accessed `/etc/passwd` | Deployed to prod, found in incident | Blocked in CI |
| Agent called unknown API | No visibility | Flagged in PR |
| New tool added to agent | Trust the vibes | Explicit approval required |
| Audit asks "what did your AI do?" | Panic | Evidence bundle with cryptographic proof |

## Quick Start

```yaml
- uses: Rul1an/assay/assay-action@v2
```

That's it. Zero config.

**What happens:**
1. Finds evidence bundles in your repo (`.assay/evidence/*.tar.gz`)
2. Verifies and lints them
3. Uploads results to GitHub Security tab
4. Comments on PR if issues found

## Example Output

### GitHub Security Tab

![Security Tab](docs/security-tab.png)

Assay findings appear alongside CodeQL, Dependabot, etc.

### PR Comment (only if issues)

```markdown
## 🛡️ Assay Evidence Report

| Check | Status |
|-------|--------|
| Verified | ✅ 3/3 bundles |
| Lint | ⚠️ 2 warnings |

<details>
<summary>Findings</summary>

| Severity | Rule | Location |
|----------|------|----------|
| ⚠️ | filesystem-sensitive | bundle.tar.gz:event-42 |
| ⚠️ | network-unknown-host | bundle.tar.gz:event-87 |

</details>
```

## Inputs

| Input | Default | Description |
|-------|---------|-------------|
| `bundles` | `**/*.tar.gz` | Glob pattern for evidence bundles |
| `fail_on` | `error` | Fail threshold: `error`, `warn`, `info`, `none` |
| `sarif` | `true` | Upload to GitHub Security tab |
| `comment_diff` | `true` | Post PR comment (only if findings) |

## Outputs

| Output | Description |
|--------|-------------|
| `verified` | `true` if all bundles verified |
| `findings_error` | Count of error-level findings |
| `findings_warn` | Count of warning-level findings |

## Permissions

```yaml
permissions:
  contents: read
  security-events: write  # For SARIF upload
  pull-requests: write    # For PR comments (optional)
```

## How Evidence Bundles Work

Evidence bundles are created when you run your AI agent with Assay:

```bash
# During tests
assay run --policy policy.yaml -- pytest tests/

# Creates: .assay/evidence/run-20260128-123456.tar.gz
```

The bundle contains:
- Every tool call your agent made
- Timestamps and sequence
- Cryptographic proof of integrity

## Advanced Usage

### Baseline Comparison

Detect regressions against your main branch:

```yaml
- uses: Rul1an/assay/assay-action@v2
  with:
    baseline_dir: .assay/baselines
    baseline_key: unit-tests
```

### Custom Threshold

Allow warnings but fail on errors:

```yaml
- uses: Rul1an/assay/assay-action@v2
  with:
    fail_on: error  # 'warn' would fail on warnings too
```

### Skip SARIF Upload

If you only want PR comments:

```yaml
- uses: Rul1an/assay/assay-action@v2
  with:
    sarif: false
```

## FAQ

**Q: What if I don't have evidence bundles yet?**

The action exits gracefully with a helpful message. No failure.

**Q: Will this spam my PRs?**

No. Comments only appear when there are findings or changes from baseline.

**Q: How is this different from CodeQL?**

CodeQL scans your code. Assay scans what your AI agent *did* at runtime.

**Q: Does this work with any AI framework?**

Yes. Assay works with traces from OpenAI, Anthropic, LangChain, or any MCP-compatible agent.

## License

MIT
