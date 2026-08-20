# Unguarded published claims

Same defect as the adequacy numbers in `INDEX.md` and
`privileged-mcp-action-v0/ERRATA.md`, looked for everywhere those files are not.

A number published in a document, produced once by hand, that nothing re-derives.
The first change to a declared source makes the document silently wrong. Today
that has been found in three adequacy rows; the prior is that it is the default
shape, not a local accident.

This file does not change those rows. Other agents own `conformance/` except this
file, plus `.github/workflows/` and `scripts/ci/`. Findings there are reported,
not edited.

**Not "almost everything is fine."** Several published claims are already false
on this tree. The check that could have shown otherwise is named on each row.

Cheap counts below were taken on 2026-08-19 against worktree
`/Users/roelschuurkes/assay` at commit
`9382ff83191f21a5c5816a1981456153f1fba8ec` (`codex/corpora-underdeclaration`).
`corpus_adequacy.py` was not run. `pgrep -lf '[c]orpus_adequacy'` was empty
before any git write in the sibling tool repo.

## What "guarded" means here

This repository already has the standard. A claim is guarded when a generator,
test, or hook would fail if the published number stopped being true of the
source it names. Two hooks are the ones to measure the rest against:

| Guard | What it actually proves |
|---|---|
| `docs-generated-drift` (`scripts/ci/check-docs-generated-drift.sh`) | Listed output files byte-match what the listed generators write in a scratch copy of the *current* tracked tree. It does **not** prove a generator derived those bytes from the codebase. A heredoc that ignores `cargo metadata` will reproduce forever. |
| `ci-programme-truth` | `AGENTS.md` still says no programme is active, and the named programme-truth inputs still match the workflow/ruleset contract. It does not fetch GitHub to see whether #2388 is closed. |

Sibling guards that meet the same bar for their own numbers:

| Guard | Re-derives |
|---|---|
| `scripts/docs/generate-agent-golden-path.py` + skill/contract tests | Workspace version, the nine golden-path steps, packaged skill copies |
| `scripts/docs/generate-product-capabilities.py` | `docs/reference/product-support.md` and `docs/generated/product-claim-proof.md` from `docs/data/product-capabilities.v0.json`. The "Published v5.3.0" headings are the last *proof-backed* release, not the workspace version. That is a guarded distinction, not drift. |
| `scripts/docs/generate-crate-deps.sh` | Workspace crate *nodes* and path-dep *edges* from `cargo metadata` (with a filter hole, row 6) |
| `scripts/ci/check-tag-tree-outward-truth.sh` | Candidate tag / source version identity |
| `scripts/ci/check-public-crate-policy.sh` | The 15 publishable crates and the 7 that are not, against manifests **and** `publish_idempotent.sh`'s `CRATES` array |
| `scripts/ci/check-msrv-policy.sh` | Public MSRV 1.89.0 |
| `scripts/ci/check-assay-version-line.sh` | Release-parser Ruby/Psych pins in `docs/reference/release.md` |
| `crates/assay-cli/tests/cli_json_identities.rs` | The twelve unnamed CLI JSON documents |
| `docs/PINNED-ACTIONS.md` | Deliberately restates **no** SHAs. The callsite is the pin. That is the opposite of this defect. |

`docs/generated/` is therefore not automatically fine. Three of five files
there are in the drift list and really derived. Two are not. See rows 5–6.

## Ranked table

Rank is `(harm if wrong) × (cheapness to guard)`. Harm is what a reader would
*do* with the number: ship in the wrong order, treat a published crate as
internal, trust a generated graph that omitted a crate. Cheapness is a
mechanical comparison to a source that already exists, not a new measurement
programme.

`Guard cost` is honest. Most rows below the cut are not worth a hook.

| Rank | Claim | Where | Re-derived? | True on this tree? | Guard cost |
|---|---|---|---|---|---|
| 1 | Publish order: `assay-common → assay-evidence → … → assay-cli` (**9 crates**) | `CLAUDE.md:220`, `docs/AIcontext/CLAUDE.md` | **No.** `publish_idempotent.sh` validates its own 15-crate array and says *“CLAUDE.md documents that edge. Nothing compared the two.”* | **No.** Script order is 15 crates and inserts `assay-registry`, `assay-canonical`, `assay-runner-schema`, `assay-adapter-api`, `assay-runner-linux`, `assay-runner-core`. | Trivial: the script already has the list. Diff CLAUDE's arrow chain against `CRATES`. Highest harm: an agent publishing from the agent file ships in an order that has already failed a release. |
| 2 | Assay-Runner is `publish = false` | `README.md:151`; `crates/assay-runner-{core,linux,schema}/src/lib.rs` (*“until Slice 7”*) | **No.** Public-crate policy lists all three as publishable and explains why `publish = false` made `assay-cli` itself unpublishable. | **No.** None of the three manifests set `publish = false`. They have published since v3.11.2. | Trivial: reuse `check-public-crate-policy.sh`. Front-page product claim plus rustdoc. |
| 3 | Workspace version `3.9.0`; leaf crates include `assay-evidence` and `assay-registry`; CLI lives in `args.rs` | `docs/AIcontext/CLAUDE.md:13,108,267` and the crate tree | **No.** Root `CLAUDE.md` was updated; this copy was not. `tag-tree-outward-truth` does not read this file. | **No.** Workspace is `5.4.0`. Those two crates have internal deps. Args are `args/mod.rs`. Crate tree predates adapters, canonical, runners, `gateway-evidence-replay`. | Cheap: generate from root `CLAUDE.md`, or delete the copy, or fail if the version line ≠ workspace version. High harm: any agent that reads `docs/AIcontext/` first. |
| 4 | `22` workspace packages, `21` under `crates/`, `7` `publish = false` | `CLAUDE.md:13-14` | **No.** Public-crate policy guards the *sets*, not these three integers or this prose. | **Yes** (22 members, 21 `crates/*`, 7 `publish = false`). | Cheap: `cargo metadata` + the policy arrays. High harm the day a crate is added; currently true, which is how this defect hides. |
| 5 | Ten-crate "module summary" living under `docs/generated/` | `docs/generated/module-summary.txt`; written by `scripts/docs/generate-module-map.sh:105-123` | **No.** The script writes a hardcoded table. The file is **not** in `GENERATED` in `check-docs-generated-drift.sh`, so even script↔file drift is uncaught. | **No** as an inventory. 10 of 22 packages; omits adapters, canonical, registry, runners, python SDK, `gateway-evidence-replay`. | Cheap to add to the drift list, and that would only pin the heredoc. Deriving the table from `cargo metadata` is a small script. Medium-high harm: the directory name is the claim that someone already did this. |
| 6 | Crate graph grouping, and the workspace member filter | `scripts/docs/generate-crate-deps.sh`; `docs/generated/crate-deps.mermaid` | **Partial.** Nodes and path-dep edges come from `cargo metadata`. Then `grep '^assay'` drops `gateway-evidence-replay`, and a trailing heredoc hardcodes four subgraphs that omit adapters, runners, canonical. Drift check will keep reproducing the omission. | **No** as a complete graph: `gateway-evidence-replay` is absent. `assay-it` is present. | Cheap: drop the name filter; use workspace members. The hardcoded groupings are the same defect as row 5 inside a file that otherwise is generated. |
| 7 | Module map of six crates | `docs/generated/module-map.mermaid` (in the drift list) | Drift check proves the file matches the heredoc in `generate-module-map.sh`. The heredoc does not read the tree. | **No** as a map of this repo. Same omissions as row 5, plus stale internal paths (`args.rs`, `jcs` still under evidence). | Same as row 5. The interesting finding is the guard that passes: this is a self-check that cannot fail when code moves. |
| 8 | Adequacy scores: `6 of 10`, `6 of 25` / `5 of 27`, `14 of 23`, `51 of 54` | `conformance/INDEX.md:148-151`; `privileged-mcp-action-v0/ERRATA.md` (`5 of 27`); pack README | **No**, which is why this file exists. ERRATA is pinned to a corpus digest that does not move when the verifier moves. Other agents are adding a re-derivation for these rows. | Not re-measured here (`corpus_adequacy.py` mutates sources; do not run it in a shared tree). INDEX and ERRATA already disagree on the privileged-action numerator and denominator (`6 of 25` vs `5 of 27`). | Owned elsewhere. Reported only. Highest documentary harm in the repo; the fix is in progress. |
| 9 | `14-vector` corpus, `5` accept, `9` reject | `README.md:172-173`; `conformance/INDEX.md:22` | **No** document-level check. The vectors exist; nothing asserts these three integers against `vectors/`. | **Yes.** 14 `*.tar.gz`, 5 `ok-*`, 9 `bad-*`. | Trivial directory count. Medium harm: this is the public reproduction invitation. |
| 10 | CLI has `~40` subcommands | `CLAUDE.md:65` | **No.** | **Imprecise.** Top-level `Command` has **34** variants. Nested `Subcommand` enums in assay-cli sum to **101**. `~40` only works if it means "top-level, roughly." | Cheap to count `Command` variants; only worth it if the prose stops being an approximation. Medium harm. |
| 11 | Sim exit `2` is infra/panic/timeout; lint has no exit `3` | `CLAUDE.md:326-332`; `docs/AIcontext/CLAUDE.md` | Partial: `exit_codes/core.rs` covers `assay run`. No table-wide contract for sim/lint. | **No / incomplete.** `commands/sim.rs` returns `2` for *time budget exceeded* (ADR-024); config uses `EXIT_CONFIG_ERROR` via `process::exit`. Lint documents exit `3` as pack-load error. | Cheap: one test that the three columns still match the three modules. Medium harm for CI scripts that treat `2` as infra. |
| 12 | `ErrorCode` (`28+` codes) | `CLAUDE.md:113` | `spec_reason_code_registry.rs` parses the enum; nothing syncs the floor in prose. | **Yes** as a floor (30 variants). A floor never goes stale upward, which is why it looks like a guard. | Cheap to print the count. Low-medium harm. Prefer deleting the number and pointing at the enum. |
| 13 | `VerifyLimits` 100MB / 1GB / 100k events | `CLAUDE.md:111` | Code defaults in `limits.rs`; tests use `VerifyLimits::default()`; no test asserts all three numbers together. | **Yes.** | Cheap, low value. The code is the authority. Copying defaults into agent context is the risk, not the absence of a fourth test. |
| 14 | `8` integrity attack vectors | `CLAUDE.md:177` | Eight `run_attack*` calls exist; no `len == 8` assert. | **Yes.** | Cheap, low value. Same shape as 13. |
| 15 | Leaf crates (7 names, no internal deps) | `CLAUDE.md:132` | `crate-deps.mermaid` would show an edge if one grew, if the crate survived the name filter (row 6). The prose list is unguarded. | **Yes** for the seven named. `docs/AIcontext/CLAUDE.md` names a different five and is wrong (row 3). | Cheap via `cargo metadata`. Medium harm only because agents treat the list as routing advice. |
| 16 | `^crates/` matched all `21` crates | `CLAUDE.md:309-310` | `perf_bench_relevance.py` now uses the metadata closure. The "21" is a historical rationale. | **Yes** as history (21 `crates/*/Cargo.toml`). False the day it is read as a current inventory. | Not worth a live guard. Date the sentence or delete the count. |
| 17 | `98.2%` filesystem, `1.31x` / `1.03x` / `2.03x` spread, `37k` WAL pairs | `CLAUDE.md:295-301`; `docs/PERFORMANCE-ASSESSMENT.md` (with provenance: 2026-08-07, local APFS) | **No**, and there should not be a CI re-derivation. These are machine-dependent one-time measurements. | Not re-run. The assessment file dates them; `CLAUDE.md` presents them as standing design facts. | **Do not guard as live numbers.** The cheap fix is to keep the dated record in PERFORMANCE-ASSESSMENT and stop promoting it into the agent file. Re-running benches on every commit would pin a different disk, which is the failure the text already describes. |
| 18 | Tool-decision latency `0.771ms` p50 / `1.913ms` p95 (and fast-path pair) | `README.md:149` | **No.** | Not re-run. One-time M1 Pro harness. | Same as 17. Front-page, so higher quote-harm, still wrong to re-derive in CI. |
| 19 | Replay/metrics speed and consistency percentages | `docs/concepts/replay.md`, `metrics.md`, `traces.md` | **No.** | Marketing estimates (`1-10 ms`, `~85-95%`). | Not worth guarding. |
| 20 | Runner extraction: `15` readiness criteria, `11` blocking conditions | `docs/reference/runner/phase-2d-consolidation-audit.md`, `extraction-roadmap.md` | **No.** Tables are hand-counted. | Not checked row-by-row. | Not worth a hook. Project-status prose. |
| 21 | RGE-Bench `71` / `95` vectors and two digests | `README.md:163` | External repository. | Not checked here. | Not this repo's job. |
| 22 | Workspace version `5.4.0` in root `CLAUDE.md` | `CLAUDE.md:13` | **No** for this file. Golden-path / tag-tree / README `v5.4.0` **are** guarded. | **Yes.** | Cheap if row 4 is done. Low extra harm while the other surfaces stay honest. |

## `docs/generated/` — verified, not assumed

| File | Derived from the tree? | In the drift list? |
|---|---|---|
| `agent-golden-path.json` | Yes — generator reads workspace version and the contract | Yes |
| `product-claim-proof.md` | Yes — from `docs/data/product-capabilities.v0.json` | Yes |
| `crate-deps.mermaid` | Yes for nodes/edges, with the `^assay` filter hole; no for the grouping subgraphs | Yes |
| `module-map.mermaid` | **No** — heredoc | Yes (script↔file only) |
| `module-summary.txt` | **No** — heredoc | **No** |

The directory name is doing the same work ERRATA's digest pin was asked to do:
imply that a later change would be noticed. For the last two files it would not.

## CLAUDE.md specifically

Load-bearing for every agent. Of the numbers you named:

- `22 / 21 / 7` — currently true, unguarded (row 4).
- `~40` subcommands — unguarded, only roughly true (row 10).
- `28+` error codes — unguarded floor, currently 30 (row 12).
- `98.2%`, `1.31x`, `1.03x` — unguarded one-time measurements, presented as
  standing facts (row 17).
- Publish order — **already false** (row 1).

Nothing in `tests/` or `scripts/` asserts any of those CLAUDE.md literals. The
search was for `22 workspace`, `28+`, `~40`, `98.2%`, `1.31x`.

## What is not worth a hook

Most of the table. Dated benchmark spreads, concept-doc estimates, runner
extraction checklists, external-repo vector counts, and floors like `28+` cost
more in false alarms than they save. The cheap work is rows 1–7 and 9–11: claims
that are already false, or that agents will treat as routing instructions, and
that already have a source of truth next to them.

The adequacy rows (8) are the same defect at higher stakes. They are owned by
the other change on this branch.

## corpus-adequacy (sibling repo)

Disjoint work, in `/Users/roelschuurkes/corpus-adequacy`, not here.

A SHA pin is exact and opaque. The tool had no version constant, no tag, and no
changelog, so a report quoted into `INDEX.md` could not name the edition that
produced it.

**Choice:** one `VERSION` constant (`0.1.0`). Every report carries `tool_version`
and, when the checkout can resolve `HEAD`, `tool_commit`. `--version` prints the
same pair. `CHANGELOG.md` names that version. The git tag is `v` plus the same
number.

Not tag-only: a pasted report would still be a SHA. Not SHA-only in the report:
a human quoting a measurement still could not name the edition. Both fields
come from one function so they cannot disagree about what "this run" was.

CI should keep pinning the commit. The version is what the report can say.
