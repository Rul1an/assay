# CLI Command Grouping RFC

Status: accepted direction; Tier 1 MCP pilot implemented

Owner: CLI / Product

Last updated: 2026-06-01 (Tier 2 revised)

## Summary

Assay should not do a big-bang command restructure. The current flat CLI is
usable, and the high-frequency commands should stay flat. The useful next step
is selective noun-verb grouping for families that already behave like resource
groups:

- `mcp` first (implemented as the Tier 1 pilot)
- `trust` second only after one more usage/docs check
- a narrowed `policy` grouping (`generate`/`record`, not the full set first
  drafted) and a corrected `replay` grouping (not `evidence`) only if user
  feedback or nearby maintenance work justifies it

Tier 2 was revised after checking the real command tree: the earlier
`policy generate/coverage/explain/fix` and `evidence bundle/replay/import`
sketches hit name collisions (`policy migrate`, `evidence import`) and a
miscategorization (replay bundles are not evidence bundles). See the Tier 2
sections for the corrected, smaller scope.

The migration contract copies the proven `trustcard` to `trust-card`
pattern from #1454: new canonical spelling, old spelling kept as a hidden
compatibility path, a stderr deprecation warning, tests for both paths, and no
artifact/output-shape changes.

## Why This Exists

The CLI has grown into a broad command surface. The quick UX fixes around help
text, trace replay errors, positional validation config, run JSON output, and
`trust-card` naming improved the immediate experience. What remains is not a
bug; it is gradual discoverability erosion.

For humans and agents, a large flat command list is harder to explore. A
selective noun-verb structure gives a predictable path:

```text
assay --help
assay mcp --help
assay mcp discover --help
```

That is easier to reason about than scanning many top-level peers. But
over-grouping would make the most common paths worse, so this RFC keeps the
main evaluation loop flat.

## Goals

- Reduce CLI discovery cost for related command families.
- Preserve existing scripts through hidden compatibility paths.
- Keep high-frequency commands short and stable.
- Avoid artifact, schema, exit-code, stdout/stderr, and output-shape churn.
- Make each future grouping reviewable as one small family PR.

## Non-Goals

- No big-bang 36-command restructure.
- No immediate code migration in this RFC.
- No removal of old command names before a future major release.
- No change to Trust Card artifact names such as `trustcard.json`.
- No forced noun-verb shape for universal commands like `run`, `doctor`, or
  `version`.
- No attempt to minimize the top-level command count as an end in itself.

## Current Shape

The current command surface is mixed: some nouns already exist, while several
related actions remain flat.

| Domain | Current commands | Current shape |
| --- | --- | --- |
| Core eval loop | `run`, `ci`, `validate`, `watch` | Flat |
| Scaffolding | `init`, `init-ci`, `setup`, `demo` | Flat |
| Policy authoring | `policy`, `generate`, `record`, `coverage`, `explain`, `fix`, `migrate`, `calibrate` | Mixed |
| Trust artifacts | `trust-basis`, `trust-card`, `baseline` | Flat |
| Evidence and replay | `evidence`, `bundle`, `replay`, `import` | Mixed |
| MCP runtime | `mcp` with hidden legacy shims for `discover`, `kill`, `tool` | Grouped |
| Runtime/security | `monitor`, `sandbox`, `quarantine`, `sim` | Mixed |
| Trace/profile data | `trace`, `profile` | Flat |
| Meta | `doctor`, `version` | Flat |

## Proposed Direction

### Keep Core Commands Flat

These commands should remain top-level:

- `assay run`
- `assay ci`
- `assay validate`
- `assay watch`
- `assay init`
- `assay doctor`
- `assay version`

These are high-frequency or universal CLI verbs. Moving them under another noun
would increase friction for the most common paths.

### Tier 1: Group MCP

Target shape:

```text
assay mcp discover
assay mcp kill
assay mcp wrap
assay mcp tool sign
```

Why first:

- `discover` and `kill` are already MCP-specific by description and behavior.
- `mcp` already exists as a hidden noun for wrapper work.
- This improves agent-oriented help exploration without touching the core eval
  loop.
- The affected commands are lower-frequency than `run`, `validate`, and `ci`.

Migration rule:

- Keep `assay discover`, `assay kill`, and any existing flat MCP spellings as
  hidden compatibility shims.
- Emit a stderr deprecation warning when a legacy flat path is used.
- Do not change policy enforcement, output files, exit codes, or JSON shapes.

Status:

- Implemented for `discover`, `kill`, and `tool` as the first grouping pilot.
- `assay mcp wrap` and `assay mcp config-path` remain in the same family.

### Tier 1: Consider Trust After MCP

Target shape:

```text
assay trust basis
assay trust card
```

Why it is a candidate:

- `trust-basis` and `trust-card` are one conceptual family.
- #1454 already proved the command alias/deprecation pattern on this surface.
- Trust Basis output behavior must remain unchanged: stdout by default, or the
  caller-supplied `--out` path, commonly documented as `trust-basis.json`.
- Trust Card artifact names must remain unchanged: `trustcard.json`,
  `trustcard.md`, and `trustcard.html`.

Why it should remain conditional:

- `trust-basis` and `trust-card` may already be clear enough as paired
  hyphenated top-level commands.
- Before moving them, check docs, examples, scripts, and user-facing material
  for direct use of both command names.
- Only group them if the help/discovery gain is worth carrying two legacy
  compatibility paths.

Migration rule:

- Keep `assay trust-basis` and `assay trust-card` as hidden compatibility
  paths.
- Emit a stderr deprecation warning when legacy paths are used.
- Keep Trust Basis output behavior and Trust Card artifact contracts unchanged.

Open question:

- `baseline` should stay flat unless future work shows it belongs under
  `trust`. It is related to scoring baselines, not necessarily Trust Basis/Card
  artifacts.

### Tier 2: Consider Policy Authoring (narrowed)

> **Revision note:** an earlier draft of this section proposed
> `policy generate / coverage / explain / fix`. Checking the real command
> tree narrowed that set: `policy` already exposes `validate`, `migrate`, and
> `fmt`, and several proposed verbs either collide or do not belong under
> `policy`. The viable Tier 2a surface is smaller than first drafted.

Viable target shape:

```text
assay policy generate   # was: generate  ("Learning Mode: Generate policy from trace")
assay policy record     # was: record    ("Learning Mode: Capture and Generate in one flow")
```

Optional, weaker fit:

```text
assay policy coverage   # was: coverage  (reports both policy and trace coverage)
```

Do **not** move these under `policy`:

- `migrate` — collides with the existing `policy migrate` ("v1.x constraints
  to v2.0 schemas"). The top-level `migrate` also handles config formats, so
  this is a semantics-merge decision, not a grouping shim. Leave both as-is
  until someone deliberately unifies the two migrate behaviors.
- `explain` — explains a test result or trace decision, not policy. It does
  not belong under `policy`. Leave flat (or revisit under a different noun).
- `fix` — "apply supported automatic fixes" is broader than policy and may
  touch config or trace fixes. Leave flat until its scope is bounded.
- `calibrate` — calibrates scoring thresholds, not policy.

Why not first:

- Moving top-level commands into a subcommand needs shim commands, not just
  clap aliases.
- This creates docs and example churn.
- The old top-level verbs must be actively supported, warned, and tested for
  at least two minor releases.

Trigger to start:

- A future policy-authoring refactor that already touches `generate`/`record`.
- User confusion around policy authoring.

### Tier 2: Group Replay (corrected — not under Evidence)

> **Revision note:** an earlier draft proposed folding `bundle`, `replay`, and
> `import` under `evidence`. Checking the real command tree showed this is
> wrong on two counts, so the target noun changed from `evidence` to `replay`.

Why the earlier `evidence` plan does not work:

- `evidence` already exposes `import` (`evidence import` for CycloneDX,
  OpenFeature, Mastra, and Pydantic evidence). The top-level `import`
  ("external artifacts into Assay-compatible data") is a **different** command,
  so `import` cannot move under `evidence` without a name collision. Leave
  top-level `import` flat. Whether the two imports should be unified is a
  separate semantics question, not a grouping move.
- `bundle` and `replay` operate on **replay bundles** ("Create replay bundle
  from run artifacts"), not **evidence bundles**. Folding them under `evidence`
  would conflate two distinct bundle concepts.

Corrected target shape — a dedicated `replay` noun:

```text
assay replay bundle create   # was: bundle create
assay replay bundle verify   # was: bundle verify
assay replay run             # was: replay  (run a recorded replay bundle)
```

Extra care vs the MCP grouping:

- This promotes the existing top-level `replay` **command** into a `replay`
  **noun**. `assay replay <bundle>` must keep working as a shim that maps to
  `assay replay run <bundle>`. That command-to-noun promotion is slightly more
  involved than the MCP case (where `discover`/`kill`/`tool` were already
  distinct commands) and deserves its own parse and behavior tests.

Trigger to start:

- A future replay or bundle UX pass that already touches this code.
- Repeated confusion between replay bundles and evidence bundles.

## Migration Contract

Every future grouping PR should follow this contract:

1. Add the new noun-verb path as canonical.
2. Keep the old path working as a hidden compatibility path.
3. Emit a concise deprecation warning to stderr on the old path.
4. Add parse tests for both new and old paths.
5. Add contract tests proving the old path still produces the same output.
6. Do not rename artifacts, schemas, receipt types, exit codes, or output
   formats.
7. Keep stdout behavior unchanged; warnings go to stderr only.
8. Keep docs focused on the new canonical path.
9. Keep the old path hidden from help output unless there is a deliberate
   visible deprecation reason.
10. Leave historical architecture/RFC references alone unless they are actively
   misleading.
11. Keep the compatibility path for at least two minor releases, and remove it
   only on a future major release.

## Implementation Notes

For a rename at the same command level, a clap alias can be enough:

```rust
#[command(name = "trust-card", alias = "trustcard")]
TrustCard(TrustCardArgs),
```

For a move from a flat command into a nested command, a clap alias is usually
not enough. The old top-level path should become a shim command that delegates
to the new handler and prints the deprecation warning.

That difference is why this RFC recommends starting with one family at a time.

## Suggested Sequence

1. Land this RFC as docs-only. Done.
2. Land the small MCP-only grouping pilot. Done.
3. If MCP grouping lands cleanly in a release, consider a trust grouping PR.
4. Defer the narrowed Tier 2 work until there is user feedback or nearby
   maintenance work:
   - Tier 2a: `policy generate` / `policy record` (only `generate`/`record`,
     plus optional `coverage`). Do not move `migrate`/`explain`/`fix`/
     `calibrate`.
   - Tier 2b: a `replay` noun (`replay bundle`, `replay run`). Not `evidence`.
     Leave top-level `import` flat (collides with `evidence import`).
5. Do not group core commands.
6. Treat each Tier 2 family as its own PR; the collisions found while drafting
   Tier 2 are exactly why a bundled restructure is unsafe.

## Review Checklist For Future Grouping PRs

- Does `assay --help` show only the canonical new path?
- Does the old path still execute successfully?
- Does the old path print a deprecation warning?
- Are output files byte-for-byte compatible where expected?
- Are stdout/stderr conventions unchanged except for the warning?
- Are current docs updated without rewriting historical context?
- Are CI workflows and scripts checked for hardcoded old paths?
- Is the PR scoped to one family?

## References

- [Command Line Interface Guidelines](https://clig.dev/)
- [CLI Guidelines](https://github.com/cli-guidelines/cli-guidelines)
- [.NET command-line design guidance](https://learn.microsoft.com/en-us/dotnet/standard/commandline/design-guidance)
- [Docker CLI deprecated features](https://github.com/docker/cli/blob/master/docs/deprecated.md)
- [Writing CLI Tools That AI Agents Actually Want to Use](https://dev.to/uenyioha/writing-cli-tools-that-ai-agents-actually-want-to-use-39no)
