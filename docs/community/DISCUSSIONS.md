# GitHub Discussions — pinned topics & live threads

Assay uses [GitHub Discussions](https://github.com/Rul1an/assay/discussions) for category clarity and roadmap Q&A. Maintainers should **pin** the threads below so newcomers see the north star without scrolling the README.

## Live discussions (March 2026)

| Topic | URL |
|-------|-----|
| What Assay is (and is not) | https://github.com/Rul1an/assay/discussions/931 |
| Trust compiler roadmap: T1a → T1b → G3 → P2 | https://github.com/Rul1an/assay/discussions/932 |
| Examples: bundle → Trust Basis → Trust Card | https://github.com/Rul1an/assay/discussions/933 |
| Outward-facing docs: trust compiler positioning (polish) | https://github.com/Rul1an/assay/discussions/934 |

Pin **931–933** (and optionally **934**) in the GitHub UI: Discussions → open thread → **Pin** (repository write access required).

---

## Seed text (same content as posted; edit in-repo to match future edits)

---

## 1. What Assay is / is not (category + non-goals)

**Suggested title:** `What Assay is (and is not)`

**Suggested body:**

**Assay is** a **trust compiler for agent systems**: it turns runtime signals (MCP tool decisions, traces, bundle contents) into **enforceable policy outcomes** and **reviewable trust artifacts** — verifiable evidence bundles, Trust Basis (`trust-basis.json`), Trust Card (`trustcard.json` / `trustcard.md` / `trustcard.html`), SARIF, CI gates.

**The wedge** many people meet first is **deterministic MCP policy enforcement** (`assay mcp wrap`): allow/deny before tools run, with an audit trail. That is the control plane, not the whole category. The `mcp` command group is **hidden** from top-level `assay --help` while the surface stabilizes; run `assay mcp --help` or follow the [MCP Quickstart](https://github.com/Rul1an/assay/tree/main/examples/mcp-quickstart).

**Assay is not:**

- A generic observability or tracing dashboard product
- An eval-as-a-service platform
- A primary “trust score” or `safe/unsafe` badge (we use explicit evidence **levels**: verified, self_reported, inferred, absent)
- A substitute for OS/network isolation — containment signals are **visible** in evidence where supported, not a proof of sandbox correctness by themselves

**Useful links:** [ADR-033](https://github.com/Rul1an/assay/blob/main/docs/architecture/ADR-033-OTel-Trust-Compiler-Positioning.md), [README](https://github.com/Rul1an/assay/blob/main/README.md).

---

## 2. Trust Compiler roadmap (plain language)

**Suggested title:** `Trust compiler roadmap: T1a → T1b → G3 → P2`

**Suggested body:**

High-level sequence (see [RFC-005](https://github.com/Rul1an/assay/blob/main/docs/architecture/RFC-005-trust-compiler-mvp-2026q2.md)):

| Wave | What shipped (intent) |
|------|------------------------|
| **T1a** | Canonical **Trust Basis** compiler output from verified bundles (`assay trust-basis generate`). |
| **T1b** | **Trust Card** — `trustcard.json` + `trustcard.md` + static `trustcard.html` from the same basis (`assay trustcard generate`). |
| **G3** | Bounded **authorization context** on supported MCP decision evidence (`auth_scheme`, `auth_issuer`, `principal`) — visibility, not auth-validation. Trust Card schema bumped; **seven** trust claims — consumers key by **`id`**, not fixed row count. |
| **P2** | Protocol-oriented claim packs (next). |

Ask questions about sequencing and boundaries here; keep feature requests in Issues.

---

## 3. Evidence & Trust artifact examples

**Suggested title:** `Examples: bundle → Trust Basis → Trust Card`

**Suggested body:**

**Minimal flow:**

1. Produce or use a **verified** evidence bundle (`.tar.gz`).
2. `assay trust-basis generate bundle.tar.gz` → `trust-basis.json` (deterministic claim rows).
3. `assay trustcard generate bundle.tar.gz --out-dir ./out` → `out/trustcard.json`, `out/trustcard.md`, `out/trustcard.html`.

**What to look for:**

- Claims are identified by **`id`** (e.g. `bundle_verified`, `authorization_context_visible`). Do not assume a fixed number of rows without checking `schema_version`.
- Trust Card **non-goals** are explicit (no aggregate score, no safe/unsafe badge as primary output).

Share redacted examples or ask for help interpreting levels (`verified` vs `absent`) here.

---

## Maintainer note

After creating these threads, set **pin** on the three that best match this outline so the Discussions landing page reinforces the same story as the README.
