# Outward Product Truth v1

Status: proposed

Measured baseline:

- repository: `Rul1an/assay`
- source: `origin/main` at `e07b93d44dad5e559d4660c92387641f13898189`
- latest published release: `v5.1.0`
- release date: 2026-08-10

## 1. Purpose

Assay's active public documentation must describe behavior that a user can obtain from the latest
published release. The documentation currently mixes released behavior, unreleased `main` behavior,
historical plans, and installation channels that are not available. This specification defines how
to reconcile those surfaces without rewriting historical records or inventing a hand-maintained
capability manifest.

The work lands as three independently reviewable slices:

1. public product truth and the canonical golden path;
2. evidence vocabulary and the false Merkle claim tracked by issue #2222;
3. status, roadmap, rollout, and distribution-document cleanup.

## 2. Truth hierarchy

When sources disagree, use this order:

1. Published artifacts and the `v5.1.0` release binary define released user behavior.
2. Current production source defines `main` behavior, which must be labelled `Unreleased` when it
   differs from the release.
3. Generated CLI help defines command and flag spelling.
4. Generated manifests and golden-path data define packaged plugin, skill, and MCP configuration.
5. Accepted ADRs define claim boundaries and explicit non-claims.
6. Historical changelogs, plans, experiments, and reports describe their recorded point in time and
   are not silently modernized.

A document cannot promote an observation, planned feature, or source-only capability to a released
claim.

## 3. Surface classes

### 3.1 Active outward surfaces

These are user-facing release truth and must be reconciled:

- root `README.md` and current sections of `CHANGELOG.md`;
- pages included in `mkdocs.yml` navigation;
- active installation, quickstart, troubleshooting, CLI, release, plugin, skill, and MCP guides;
- `.mcp.json`, `.cursor/mcp.json`, `.claude-plugin/marketplace.json`, and
  `packaging/claude-plugin/**`;
- generated golden-path documentation and the source data that owns it.

### 3.2 Current repository status surfaces

Roadmaps, distribution guides, and rollout templates outside the MkDocs navigation remain visible to
repository readers. They must either state their measurement date and status clearly or be replaced
by a short tombstone pointing to current release and issue state.

### 3.3 Historical surfaces

Released changelog entries, accepted ADR history, experiment records, audit reports, and closed plans
retain their original claims. A correction must be additive and visibly dated. Rewriting the original
measurement or decision is prohibited.

## 4. Released distribution truth

At the measured baseline, the verified public channels are:

- GitHub release assets for `v5.1.0`;
- `https://getassay.dev/install.sh` for supported Unix release binaries;
- crates.io packages `assay-cli`, `assay-mcp-server`, and `assay-core` at `5.1.0`;
- PyPI package `assay-it` at `5.1.0`;
- GitHub Marketplace action `Rul1an/assay-action`;
- MCP Registry server `io.github.Rul1an/assay-mcp-server` at `5.1.0`;
- the repository's Claude plugin marketplace manifest and packaged plugin.

Active documentation must not present Homebrew, Scoop, or a GHCR image as available until a release
pipeline publishes and verifies those channels. Windows examples must derive the release asset name
from the release, not use the obsolete `assay-windows-x86_64.zip` name.

The Python SDK install command is `pip install assay-it`. `pip install assay` installs an unrelated
package and is forbidden in active Assay installation guidance.

## 5. Canonical golden path

There is one canonical release-pinned install-to-evidence path. Other quickstarts link to it instead
of copying a divergent sequence.

The path must:

1. install or select release `v5.1.0` through a verified channel;
2. print `assay 5.1.0` before relying on behavior;
3. create or use a minimal repository fixture;
4. execute Assay and produce a machine-readable result;
5. show the relevant exit status and evidence artifact;
6. state what the artifact proves and does not prove;
7. include upgrade and rollback directions.

Generated agent instructions may reuse the path, but generated files must not become a second source
of truth. The existing `docs/generated/agent-golden-path.json` remains the owner for generated agent
instructions until issue #1977 supplies a product-wide capability manifest.

`assay init --from-trace` must be described as producing a runtime-observation allowlist policy with
`files`, `network`, and `processes`. It must not be described as producing the MCP authorization
policy consumed by `assay policy validate` or MCP enforcement.

## 6. Plugin, skill, and MCP truth

The documentation distinguishes four deliverables:

- CLI: the `assay` binary;
- MCP server: the separate `assay-mcp-server` binary;
- skill: generated Assay operating instructions for an agent;
- Claude plugin: the packaged plugin containing its declared commands, skill, and MCP configuration.

Project MCP examples use the `mcpServers` wrapper. Claude uses `.mcp.json`; Cursor uses
`.cursor/mcp.json`. The stdio invocation is:

```json
{
  "command": "assay-mcp-server",
  "args": ["--policy-root", "."]
}
```

The documentation must state that `.` is interpreted relative to the host-provided working
directory. It must not claim that the host always starts the server at the repository root.

The release server exposes five production tools. `assay_test_outbound` is a test-feature tool and
must not be advertised as part of the release surface.

MCP protocol gaps tracked separately, including issue #2157, are explicit non-claims. Packaging must
not imply that the server enforces proxy policy when it is started in plain stdio mode.

## 7. Evidence vocabulary

`run_root` is SHA-256 over newline-delimited event content-hash strings, with a trailing newline,
in event sequence order.
It is not a Merkle root and it does not provide a Merkle inclusion proof.

Slice 2 replaces this false terminology in current product documentation, public code comments,
tests, demos, and fixtures that teach the public contract. Genuine Merkle references remain valid
when they describe a real Merkle construction, including Rekor, RFC 6962 experiments, and the
transparency-log ADR.

A scoped recurrence guard must reject new false `run_root`-as-Merkle claims while allowing named,
reviewed genuine uses. The guard must not ban the word `Merkle` repository-wide.

## 8. Status and maintenance policy

Current status pages use durable names and current release/issue references. Closed programme names,
old launch checklists, and stale version banners are not presented as current execution state.

For each stale status document, choose one action:

1. update it when it remains the canonical current surface;
2. replace it with a dated tombstone and links to current sources;
3. leave it unchanged and label it historical when its original content is evidence.

Do not maintain a prose capability matrix by hand. Issue #1977 owns the generated machine-readable
capability manifest. Until that exists, documentation states only individually verified channels and
capabilities with their release or measurement date.

## 9. Automated controls

`mkdocs build --strict` remains required, but it is not sufficient because it does not execute CLI
examples or validate external distribution claims.

Slice 1 adds a narrow active-doc contract that:

- reads the workspace release version from `Cargo.toml`;
- rejects known false active install commands and unsupported channels;
- pins selected canonical command spellings to generated help or existing CLI contracts;
- checks that generated golden-path files match their source;
- avoids live network calls in ordinary CI.

Slice 2 adds the scoped evidence-vocabulary guard described in section 7.

External channel availability is release-time evidence. Ordinary PR CI validates repository
references and syntax; a release checklist or scheduled probe validates network availability.

## 10. Slice boundaries and acceptance

### Slice 1: Public truth and golden path

Acceptance criteria:

- active install pages contain only verified channels;
- no active page recommends `pip install assay`;
- release asset names and version examples are correct for `v5.1.0`;
- command examples agree with release help;
- CLI, MCP server, skill, and plugin are clearly distinguished;
- one canonical release-pinned golden path owns the full sequence;
- a focused active-doc contract fails under representative command, version, package, and channel
  mutations;
- `mkdocs build --strict` passes.

### Slice 2: Evidence terminology

Acceptance criteria:

- issue #2222's false current `run_root` Merkle claims are removed;
- genuine Merkle constructions remain documented;
- the actual flat digest construction is stated once and reused by reference;
- the scoped guard fails when a false claim is reintroduced and passes for allowlisted genuine uses;
- affected tests and documentation checks pass.

### Slice 3: Status cleanup

Acceptance criteria:

- current roadmap and distribution entrypoints identify `v5.1.0` and `Unreleased` correctly;
- stale rollout and launch documents are updated, tombstoned, or labelled historical;
- no closed programme is presented as active;
- historical records retain their original substance;
- `mkdocs build --strict` passes.

## 11. Non-goals

This work does not:

- publish a new release or create a new distribution channel;
- implement issue #1977's capability manifest;
- implement MCP protocol issue #2157;
- claim certification, partnership, compliance, agent safety, provider-outcome verification, or a
  scalar trust score;
- rewrite old release notes, ADR decisions, experiments, or audit measurements;
- change CLI, evidence, policy, or MCP runtime behavior except for documentation-only test guards.

## 12. Review and landing

Each slice uses its own branch, worktree, PR, verification record, and exact-head independent review.
The slices land in order because later status cleanup may link to the canonical surfaces created by
slice 1, while slice 2 remains behaviorally independent and can be reviewed in parallel.
