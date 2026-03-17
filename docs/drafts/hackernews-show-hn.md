# Show HN submission

**Title:** Show HN: Assay – A firewall for MCP tool calls with replayable evidence

**URL:** https://github.com/Rul1an/assay

---

## First comment (post immediately)

I built Assay because a recent scan found that popular MCP servers average 34/100 on security [1], and OWASP now lists "Lack of Audit and Telemetry" as a top MCP risk [2].

There are scanners (Cisco's mcp-scanner, MCPSec) and proxies (Intercept, mcpwall) emerging, but none produce evidence you can replay. That's the gap Assay fills.

How it works: Assay wraps your MCP server as a stdio proxy. Every `tools/call` JSON-RPC message gets evaluated against a YAML policy. Allowed calls pass through. Blocked calls never reach the server. Every decision gets logged to a tamper-evident evidence bundle.

Quick try:

    cargo install assay-cli
    assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
      -- npx @modelcontextprotocol/server-filesystem /tmp/demo

What you get:

    ✅ ALLOW  read_file  path=/tmp/demo/safe.txt  reason=policy_allow
    ❌ DENY   read_file  path=/etc/passwd          reason=path_constraint_violation

Policy is a YAML file — allow/deny lists with regex constraints on arguments. You can also generate one from observed behavior (`assay init --from-trace trace.jsonl`).

What I'm most interested in feedback on: Is the evidence trail actually useful for your workflow, or is just blocking sufficient? The compliance packs (EU AI Act, SOC2) feel valuable in theory but I'm unsure if that's what individual developers want.

Technical details: Rust, ~166K LOC, MIT licensed, < 5ms overhead per tool call. Tested with 12 attack vectors across 3 security experiments (memory poisoning, delegation spoofing, protocol interpretation) with zero false positives [3].

[1] https://dev.to/elliotllliu/we-scanned-17-popular-mcp-servers-heres-what-we-found-321c
[2] https://owasp.org/www-project-mcp-top-10/
[3] https://github.com/Rul1an/assay/blob/main/docs/architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md

---

**Best timing:** Tuesday-Thursday, 8-10 AM EST
**Key: ask a genuine question in the first comment to invite discussion**
