# Assay Design Partner Program

## One-Liner

**Stop your AI agent tests from being flaky. Assay replays recorded behavior — no API calls, no network, no random failures.**

---

## The Problem We Solve

| Pain Point | What Teams Do Now | Result |
|------------|-------------------|--------|
| **Flaky CI** | Call LLMs in every test run | Random failures → developers ignore CI |
| **Slow Feedback** | Wait 3+ minutes for GPT-4 | PRs queue up, velocity drops |
| **Expensive Testing** | 50 tests × 20 PRs/day × $0.20 | $200/day, $4k/month just for tests |
| **Privacy Concerns** | Send prompts to observability platforms | Compliance blocks adoption |

---

## How Assay Fixes This

```bash
# 1. Record a successful agent session (once)
assay import --format mcp-inspector session.json --init

# 2. Run tests in CI (every PR, instant, free)
assay run --config mcp-eval.yaml --strict
```

**Result:** 3ms tests. $0 cost. 0% flakiness. 100% local.

---

## What We're Looking For

We're seeking **3-5 engineering teams** to partner with during our v1.0 development.

### Ideal Partner Profile

- [ ] Running AI agents in production (or close to it)
- [ ] Using MCP, LangChain, LlamaIndex, or custom agent frameworks
- [ ] Frustrated with test flakiness or CI costs
- [ ] Willing to provide feedback weekly for 4-6 weeks

### What You Get

| Benefit | Details |
|---------|---------|
| **Early Access** | Use v0.8.0-rc.1 before public release |
| **Direct Support** | Slack channel with core team |
| **Feature Input** | Your use cases shape the roadmap |
| **Free Forever** | Design partners get perpetual free license |

### What We Ask

| Commitment | Time |
|------------|------|
| Weekly sync (15 min) | Share what's working, what's not |
| Run Assay in staging CI | Real-world validation |
| Share anonymized metrics | Test counts, failure rates, time saved |

---

## Quick Demo

### Before Assay (Traditional)

```yaml
# .github/workflows/tests.yml
- name: Run Agent Tests
  run: pytest tests/agent/
  # ⏱️ 3-5 minutes
  # 💰 $0.50/run
  # 🎲 10% flake rate
```

### After Assay

```yaml
# .github/workflows/tests.yml
- uses: Rul1an/assay-action@v1
  with:
    config: mcp-eval.yaml
  # ⏱️ 50ms
  # 💰 $0.00
  # 🎲 0% flake rate
```

---

## Use Cases We're Validating

### 1. RAG Pipeline Testing
> "Did the agent retrieve the right documents and cite them correctly?"

**Assay approach:** Record successful retrieval → validate future runs match the sequence.

### 2. Multi-Agent Workflows
> "Did Agent A hand off to Agent B at the right time with the right context?"

**Assay approach:** Sequence rules enforce correct handoff order.

### 3. Tool Argument Validation
> "Did the agent call the API with valid parameters?"

**Assay approach:** JSON Schema validation on every tool call.

### 4. Safety Guardrails
> "Did the agent avoid calling dangerous tools?"

**Assay approach:** Blocklist enforcement, fail-fast in CI.

---

## Technical Requirements

| Requirement | Details |
|-------------|---------|
| **OS** | Linux, macOS, Windows |
| **Runtime** | Rust (native) or Python 3.9+ |
| **CI** | GitHub Actions, GitLab CI, or any runner |
| **Agent Framework** | MCP, LangChain, LlamaIndex, or custom |
| **Network** | None required (fully offline) |

---

## Timeline

| Week | Milestone |
|------|-----------|
| Week 1 | Onboarding: install Assay, import first trace |
| Week 2 | Configure policies for your specific use case |
| Week 3 | Integrate into staging CI |
| Week 4 | Measure: flakiness, speed, cost savings |
| Week 5-6 | Iterate based on edge cases |

---

## Next Steps

1. **Reply to this email** with:
   - Your agent framework (MCP / LangChain / custom)
   - Current test setup (pytest / custom / none)
   - Biggest pain point (flakiness / speed / cost / privacy)

2. **We'll schedule a 30-min intro call** to understand your setup

3. **You'll get access** to v0.8.0-rc.1 and our private Slack

---

## Contact

- **Email:** partners@assay.dev
- **GitHub:** [github.com/Rul1an/assay](https://github.com/Rul1an/assay)
- **Docs:** [docs.assay.dev](https://docs.assay.dev)

---

## FAQ

**Q: Is this production-ready?**
A: v0.8.0-rc.1 is feature-complete and hardened. We're in "soak period" before v1.0.

**Q: What if my agent framework isn't MCP?**
A: We support any JSON-based log format. LangChain/LlamaIndex adapters are in progress.

**Q: Will my data leave my network?**
A: No. Assay runs 100% locally. No telemetry, no cloud dependencies.

**Q: What's the catch?**
A: We need your feedback to build the right product. That's the only "payment."

---

*"In metallurgy, an assay determines the purity of precious metals. In software, Assay determines the quality of your AI."*
