# Outward Product Truth Slice 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make current roadmap, rollout, and distribution entrypoints accurately describe `v5.1.0`, `Unreleased`, and the remaining tracked work without rewriting historical records.

**Architecture:** Replace stale current-status narratives with short durable indexes. Preserve dated material as history, use tombstones for superseded launch checklists, and link capability work to issues #1975 and #1977 instead of copying a matrix into prose.

**Tech Stack:** Markdown, MkDocs, Python 3 standard library, the Slice 1 release-surface checker.

## Global Constraints

- Current release is `v5.1.0`; changes after the tag are `Unreleased`.
- No programme is active unless `AGENTS.md` names its ledger.
- Do not rewrite released changelog entries, accepted ADR decisions, or dated measurements.
- Do not claim a channel submission, certification, partnership, or marketplace approval without a checkable artifact.
- Do not create a hand-maintained capability matrix; issue #1977 remains its owner.
- Slice 3 lands after Slice 1 so links can target its canonical journey.

---

### Task 1: Add status-document mutation checks

**Files:**
- Modify: `scripts/ci/check-release-surface.sh`
- Modify: `scripts/ci/test-check-release-surface.sh`
- Modify: `.pre-commit-config.yaml`

**Interfaces:**
- Consumes: the Slice 1 checker and `[workspace.package].version`.
- Produces: current-status checks for the three canonical status entrypoints only.

- [ ] **Step 1: Add failing status mutations**

Extend the self-test with:

```bash
mutate_and_expect_failure \
  stale-roadmap-release \
  docs/ROADMAP.md \
  's/Current release: `v5.1.0`/Current release: `v3.36.0`/' \
  'roadmap current release drift'

mutate_and_expect_failure \
  stale-dx-status \
  docs/DX-ROADMAP.md \
  's/Status: historical roadmap/Status: active execution plan/' \
  'DX roadmap status drift'

mutate_and_expect_failure \
  stale-distribution-verification \
  docs/DISTRIBUTION-SUBMISSION-GUIDE.md \
  's/Status: historical submission record/Status: current submission checklist/' \
  'distribution guide status drift'
```

- [ ] **Step 2: Run and confirm RED**

```bash
bash scripts/ci/test-check-release-surface.sh
```

Expected: FAIL because current status markers do not exist.

- [ ] **Step 3: Extend the single release-surface checker**

Require exact derived/current markers:

```bash
grep -qxF "Current release: \`v$WORKSPACE_VERSION\`" docs/ROADMAP.md \
  || fail "docs/ROADMAP.md: roadmap current release drift"
grep -qxF "Status: historical roadmap" docs/DX-ROADMAP.md \
  || fail "docs/DX-ROADMAP.md: DX roadmap status drift"
grep -qxF "Status: historical submission record" docs/DISTRIBUTION-SUBMISSION-GUIDE.md \
  || fail "docs/DISTRIBUTION-SUBMISSION-GUIDE.md: distribution guide status drift"
```

Expand the pre-commit `files:` expression to those three docs.

- [ ] **Step 4: Run the self-test**

```bash
bash scripts/ci/test-check-release-surface.sh
```

Expected: mutation cases pass; live checker remains red until Tasks 2 and 3 add the markers. Do not commit this intermediate red repository state.

### Task 2: Replace stale roadmaps with durable status indexes

**Files:**
- Modify: `docs/ROADMAP.md`
- Modify: `docs/DX-ROADMAP.md`

**Interfaces:**
- Consumes: release `v5.1.0`, `[Unreleased]` in `CHANGELOG.md`, and open issues #1973, #1975, and #1977.
- Produces: one current roadmap index and one clearly historical DX roadmap.

- [ ] **Step 1: Make `docs/ROADMAP.md` the current index**

Start it with:

```markdown
# Assay Roadmap

Current release: `v5.1.0`

Changes merged after that tag are tracked under `[Unreleased]` in
`CHANGELOG.md`. No programme is active unless `AGENTS.md` names its public
ledger.
```

Add a short durable current index: shipped release, release-blocking issues, MVP productization (#1973/#1975/#1977), post-MVP backlog, and links to historical plans. Preserve the existing dated body under `## Historical roadmap record` instead of deleting or silently rewriting it. Do not add new per-PR completion tables.

- [ ] **Step 2: Tombstone `docs/DX-ROADMAP.md` as historical**

Prepend:

```markdown
# DX Roadmap (Historical)

Status: historical roadmap

This document records the P0-P2 DX programme as it was planned. It is not the
current execution ledger. See `ROADMAP.md`, the current release,
and open GitHub issues for present state.
```

Leave the dated body intact unless a sentence presents itself as current. Move such sentences under a “Recorded plan” heading rather than updating old completion claims.

- [ ] **Step 3: Verify current-state derivation**

```bash
bash scripts/ci/check-release-surface.sh
git diff --check
```

Expected: only the distribution status marker remains red. Do not commit while the live checker remains red.

### Task 3: Tombstone stale rollout and distribution plans

**Files:**
- Modify: `docs/DISTRIBUTION-SUBMISSION-GUIDE.md`
- Modify: `docs/guides/rollout-template.md`

**Interfaces:**
- Consumes: verified channels from Slice 1 and current roadmap from Task 2.
- Produces: dated historical records with current pointers and no unearned availability claim.

- [ ] **Step 1: Convert the distribution guide to a historical record**

Prepend:

```markdown
# Distribution Submission Guide (Historical)

Status: historical submission record

Last verified: 2026-03-17. This document records a submission plan and is not
evidence that every listed channel shipped. Current verified channels and
installation commands are in `getting-started/installation.md`.
```

Preserve the dated body under `## Historical submission plan`. Add the tombstone and current pointer without rewriting the original record. If an outward link exposes an unsupported availability claim, add a dated correction immediately above it rather than silently changing the historical text.

- [ ] **Step 2: Tombstone the rollout template**

Replace the active-looking v0.3.4 checklist with a short historical pointer:

```markdown
# Rollout Template (Historical)

This checklist belongs to the v0.3.4 launch period and is retained for
provenance. It is not the release checklist for `v5.1.0`.

- Current release process: `reference/release.md`
- Current installation: `getting-started/installation.md`
- Current roadmap: `ROADMAP.md`
```

- [ ] **Step 3: Verify status and links**

```bash
bash scripts/ci/test-check-release-surface.sh
bash scripts/ci/check-release-surface.sh
python3 -m venv /tmp/assay-docs-v51
/tmp/assay-docs-v51/bin/pip install -r docs/requirements-ci.txt
/tmp/assay-docs-v51/bin/mkdocs build --strict
```

Expected: PASS.

- [ ] **Step 4: Commit the green guard and status cleanup together**

```bash
git add -- scripts/ci/check-release-surface.sh scripts/ci/test-check-release-surface.sh .pre-commit-config.yaml docs/ROADMAP.md docs/DX-ROADMAP.md docs/DISTRIBUTION-SUBMISSION-GUIDE.md docs/guides/rollout-template.md
git commit -m "docs(status): mark superseded launch material historical"
```

### Task 4: Final repository documentation verification

**Files:**
- Modify only if needed: `mkdocs.yml` for links to the current roadmap or canonical journey.

**Interfaces:**
- Consumes: all three slices after they are merged in order.
- Produces: final outward documentation verification and review packet.

- [ ] **Step 1: Build and scan active docs**

```bash
bash scripts/ci/test-check-release-surface.sh
bash scripts/ci/check-release-surface.sh
python3 scripts/docs/generate-agent-golden-path.py --check
/tmp/assay-docs-v51/bin/mkdocs build --strict
git grep -nE 'Current release:|Status: (active|current)' -- README.md docs/ROADMAP.md docs/DX-ROADMAP.md docs/DISTRIBUTION-SUBMISSION-GUIDE.md docs/guides/rollout-template.md
git diff --check origin/main...HEAD
```

Manually classify every grep result as derived current state or a false active marker.

- [ ] **Step 2: Run repository hooks**

```bash
pre-commit run --all-files
cargo fmt --all -- --check
```

- [ ] **Step 3: Open a draft PR and request one non-building exact-head review**

The PR body lists which documents were updated, tombstoned, or preserved as history. It states that no product runtime behavior, distribution publication, certification, or marketplace status changed.
