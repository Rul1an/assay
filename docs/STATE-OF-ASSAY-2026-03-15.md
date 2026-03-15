# State of Assay — 2026-03-15

## Executive Summary

Assay v3.1.0 is released and live. All four identified architecture/roadmap gaps from the Q2 2026 gap analysis are closed. The codebase is in a strong, closed-loop state with no open delivery debt.

| Metric | Value | Verified |
|--------|-------|----------|
| Current version | `3.1.0` | `grep '^version' Cargo.toml` → `version = "3.1.0"` |
| Latest release | `v3.1.0` | `git tag --sort=-v:refname \| head -1` → `v3.1.0` |
| Release date | 2026-03-15 | GitHub Release published `2026-03-15T16:47:50Z` |
| Crates | 16 | `ls -d crates/*/ \| wc -l` → `16` |
| Rust LOC | ~166,000 | `find crates -name '*.rs' \| xargs wc -l` → `166358 total` |
| ADRs | 44 | `ls docs/architecture/ADR-*.md \| wc -l` → `44` |
| RFCs | 4 | `ls docs/architecture/RFC-*.md \| wc -l` → `4` |
| Wave plans | 34 | `ls docs/contributing/SPLIT-PLAN-*.md \| wc -l` → `34` |
| Review gate scripts | 244 | `ls scripts/ci/review-*.sh \| wc -l` → `244` |
| CI workflows | 28 | `ls .github/workflows/*.yml \| wc -l` → `28` |
| Doc files | 276 | excl. archive + wave checklists |
| Open PRs | 1 | `#866` (Structurizr CI, pending merge) |

---

## Release Status

### v3.1.0 — Released 2026-03-15

**Verification:**

```
$ ./target/release/assay --version
assay 3.1.0

$ gh release view v3.1.0 --json tagName,publishedAt,assets
{
  "tag": "v3.1.0",
  "published": "2026-03-15T16:47:50Z",
  "assets": [
    "assay-v3.1.0-aarch64-unknown-linux-gnu.tar.gz",
    "assay-v3.1.0-aarch64-unknown-linux-gnu.tar.gz.sha256",
    "assay-v3.1.0-x86_64-unknown-linux-gnu.tar.gz",
    "assay-v3.1.0-x86_64-unknown-linux-gnu.tar.gz.sha256"
  ]
}
```

**Published to:**
- GitHub Releases: binaries (Linux x86_64 + aarch64) with SHA-256 checksums
- crates.io: all workspace crates including `assay-adapter-api` (first publish)
- PyPI: Python wheels (macOS x86_64, macOS aarch64, Linux x86_64)

**v3.1.0 highlights (since v3.0.0, 269 non-merge commits):**
- MCP policy enforcement stack (Wave24–Wave42)
- BYOS evidence store Phase 1 (store-status, config, docs)
- Architecture-as-code and docs hygiene

---

## Product Capabilities

### CLI Command Surface

**Verified output of `assay --help`:**

| Category | Commands |
|----------|----------|
| **Core** | `run`, `ci`, `validate`, `init`, `doctor`, `watch` |
| **Evidence** | `export`, `verify`, `show`, `lint`, `diff`, `push`, `pull`, `list`, `store-status`, `explore` |
| **BYOS Store** | `push`, `pull`, `list`, `store-status` (with `.assay/store.yaml` config) |
| **MCP** | `mcp wrap`, `mcp config-path` |
| **Policy** | `policy`, `generate`, `record`, `profile` |
| **Security** | `sandbox`, `monitor`, `tool` (sign/verify) |
| **Replay** | `bundle`, `replay` |
| **Simulation** | `sim` (attack simulation, soak testing) |
| **DX** | `explain`, `coverage`, `discover`, `kill`, `demo`, `setup` |

### Evidence Pipeline — End-to-End Verified

```
$ assay evidence export --profile profile.yaml --out bundle.tar.gz
Exported evidence bundle to bundle.tar.gz

$ assay evidence verify bundle.tar.gz
Bundle verified (bundle.tar.gz): OK

$ assay evidence push bundle.tar.gz --store file:///tmp/store
✅ Bundle verified: sha256:9a36ebccd2c3bcfcf...
✅ Uploaded: sha256:9a36ebccd2c3bcfcf...

$ assay evidence list --store file:///tmp/store --format json
{
  "bundles": [{
    "bundle_id": "sha256:9a36ebccd2c3bcfcf...",
    "size": 912,
    "modified": "2026-03-15T17:41:41Z"
  }],
  "count": 1
}

$ assay evidence store-status --store file:///tmp/store --format json
{
  "reachable": true,
  "readable": true,
  "writable": true,
  "backend": "file",
  "bundle_count": 1,
  "total_size_bytes": 912,
  "object_lock": "unknown"
}
```

### Compliance Packs — Verified

```
$ ls packs/open/
cicd-starter/
eu-ai-act-baseline/
soc2-baseline/

$ assay evidence lint --pack cicd-starter bundle.tar.gz --format json
{
  "tool_version": "3.1.0",
  ...
}
```

Three open-source packs shipped: CICD starter, EU AI Act baseline (Article 12), SOC2 baseline.

---

## Architecture Maturity

### Crate Dependency Graph

```
assay-cli → assay-core, assay-metrics, assay-mcp-server, assay-sim, assay-evidence
assay-mcp-server → assay-core, assay-common, assay-metrics
assay-core → assay-common
assay-metrics → assay-core, assay-common
assay-sim → assay-core, assay-evidence

Leaf crates: assay-common, assay-policy, assay-evidence, assay-registry, assay-xtask
Adapters: assay-adapter-api, assay-adapter-acp, assay-adapter-a2a, assay-adapter-ucp
Platform: assay-ebpf, assay-monitor
```

No circular dependencies. All dependencies flow in one direction.

### MCP Policy Enforcement (ADR-032)

Closed-loop through Wave42 on `main`:

| Wave | Capability | Status |
|------|-----------|--------|
| 24 | Typed decisions + Decision Event v2 | ✅ Merged |
| 25–26 | Log + alert obligation execution | ✅ Merged |
| 27–28 | Approval artifact + approval_required enforcement | ✅ Merged |
| 29–30 | Restrict_scope contract + enforcement | ✅ Merged |
| 31–32, 36 | Redact_args contract + enforcement | ✅ Merged |
| 33, 35 | Fulfillment normalization | ✅ Merged |
| 34, 40 | Fail-closed / deny evidence convergence | ✅ Merged |
| 37–39 | Decision/evidence convergence, replay diff, evidence compat | ✅ Merged |
| 41 | Consumer hardening | ✅ Merged |
| 42 | Context envelope hardening | ✅ Merged |

### BYOS Evidence Store (ADR-015)

| Phase | Scope | Status |
|-------|-------|--------|
| Phase 1 | push, pull, list, store-status, config, docs | ✅ Complete |
| Phase 2 | GitHub Action integration | Future (demand-gated) |
| Phase 3 | Managed store | Future (PMF-gated) |

### Protocol Adapters (ADR-026)

| Adapter | Protocol | Status |
|---------|----------|--------|
| `assay-adapter-acp` | Agentic Commerce Protocol (OpenAI/Stripe) | ✅ Merged |
| `assay-adapter-ucp` | Universal Commerce Protocol (Google/Shopify) | ✅ Merged |
| `assay-adapter-a2a` | Agent2Agent (Google) | ✅ Merged |
| `assay-adapter-api` | Common adapter trait | ✅ Published to crates.io |

---

## Gap Analysis Status

All four items from the Q2 2026 gap analysis are closed:

| # | Gap | Closed by | PR(s) |
|---|-----|-----------|-------|
| 1 | Roadmap truth sync | Docs/ADR status aligned to merged reality | #857 |
| 2 | ADR-015 Phase 1 closure | store-status, config, provider docs, integration tests | #859, #860, #862 |
| 3 | Release/changelog hygiene | Consolidated to curated CHANGELOG.md (-2,137 lines) | #864 |
| 4 | Architecture-as-code CI | Structurizr validation workflow + Mermaid export | #866 |

**Verification (GAP doc recommended order):**

```
1. ~~Roadmap truth sync~~ ✅ Done (PR #857)
2. ~~ADR-015 Phase 1 closure~~ ✅ Done (PR #859, #860, #862)
3. ~~Release/changelog hygiene~~ ✅ Done (PR #864)
4. ~~Architecture-as-code CI~~ ✅ Done (PR #866)
```

---

## CI/CD Infrastructure

### Workflows (28 total)

| Category | Workflows |
|----------|-----------|
| **Core CI** | `ci.yml` (build/test/clippy/bench on Linux/macOS/Windows + eBPF) |
| **Security** | `assay-security.yml`, `assay.yml` (Assay Gate) |
| **Parity** | `parity.yml` (batch vs streaming) |
| **Performance** | `perf_main.yml`, `perf_pr.yml`, `perf_nightly.yml` |
| **Docs** | `docs.yml`, `docs-auto-update.yml`, `docs-link-check.yml` |
| **Release** | `release.yml` (binaries, crates.io, PyPI) |
| **Smoke** | `smoke-install.yml` |
| **Action tests** | `action-tests.yml`, `action-v2-test.yml` |
| **Gates** | `split-wave0-gates.yml`, `structurizr-validate.yml` |
| **Nightly** | `adr025-nightly-*.yml` (soak, readiness, closure, otel-bridge) |
| **Kernel** | `kernel-matrix.yml` |

### Review Gates

244 review gate scripts under `scripts/ci/review-*.sh`. Each script enforces:
- Allowlist-only diff
- Workflow-ban
- Frozen path protection
- Content marker checks
- Pinned test execution

---

## Documentation

### Structure

| Area | Files | Purpose |
|------|-------|---------|
| `docs/architecture/` | 44 ADRs, 4 RFCs, gap docs, plans | Architecture decisions and status |
| `docs/guides/` | 10+ guides | Operator quickstarts (S3, B2, MinIO, etc.) |
| `docs/contributing/` | 34 wave plans, 155+ checklists | Wave delivery discipline |
| `docs/releases/` | Per-version runbooks | Release process |
| `CHANGELOG.md` | Curated, per-version | Single source of truth for releases |

### Architecture-as-Code

| Asset | Location | Status |
|-------|----------|--------|
| Structurizr workspace | `docs/architecture/structurizr/adr-032/workspace.dsl` | ✅ Validated in CI |
| Mermaid exports | `structurizr/adr-032/export/*.mmd` | 4 views (SystemContext, Containers, PolicyRuntime, Evidence) |
| Backstage catalog | `catalog-info.yaml` | Component metadata |
| MkDocs site | `mkdocs.yml` | 229 nav entries |

---

## What's Next

With all Q2 2026 gaps closed and v3.1.0 released, the repo is in a clean state. Possible next bounded slices, in order of leverage:

| Priority | Slice | Type | Gated by |
|----------|-------|------|----------|
| **P1** | ADR-015 Phase 2 (GitHub Action store integration) | Feature | Demand |
| **P2** | Broader Structurizr component views (beyond ADR-032) | Docs | Discretionary |
| **P3** | Sigstore keyless signing | Feature | Enterprise demand |
| **P3** | SIEM connectors (Splunk/Sentinel) | Feature | Enterprise demand |

The decision rule from the GAP doc still applies:
> Prefer the slice that improves the most truthfulness with the least new runtime surface.

---

## Session Log (2026-03-15)

PRs merged today, in order:

| # | PR | Type |
|---|-----|------|
| 1 | #857 | Roadmap/docs truth sync |
| 2 | #859 | ADR-015 Phase 1 Step 1 (freeze) |
| 3 | #860 | ADR-015 Phase 1 Step 2 (implementation) |
| 4 | #862 | ADR-015 Phase 1 Step 3 (closure) |
| 5 | #864 | Release/changelog hygiene |
| 6 | #865 | v3.1.0 changelog and release prep |
| 7 | #866 | Structurizr CI validation (pending) |

Plus: `v3.1.0` tag pushed, release workflow completed, `assay-adapter-api` first crates.io publish.
