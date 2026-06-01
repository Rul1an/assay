# CLI Command Grouping RFC

Status: draft proposal

Owner: CLI / Product

Last updated: 2026-06-01

## Summary

Assay should not do a big-bang command restructure. The current flat CLI is
usable, and the high-frequency commands should stay flat. The useful next step
is selective noun-verb grouping for families that already behave like resource
groups:

- `mcp` first
- `trust` second only after one more usage/docs check
- `policy` and `evidence` only if user feedback or maintenance work justifies it

The migration contract should copy the proven `trustcard` to `trust-card`
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
| MCP runtime | `mcp`, `discover`, `kill`, `tool` | Mixed |
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

### Tier 2: Consider Policy Authoring

Possible target shape:

```text
assay policy generate
assay policy coverage
assay policy explain
assay policy fix
```

Why not first:

- `generate`, `coverage`, `explain`, and `fix` may be more familiar as flat
  commands.
- Moving top-level commands into a subcommand usually needs shim commands, not
  just clap aliases.
- This creates broader docs and example churn.
- This should only start if the old top-level verbs can be actively supported,
  warned, and tested for at least two minor releases.

Trigger to start:

- User confusion around policy authoring.
- A future policy-command refactor.
- A concrete need to make agent help traversal cleaner.

### Tier 2: Consider Evidence and Replay

Possible target shape:

```text
assay evidence bundle
assay evidence replay
assay evidence import
```

Why not first:

- `evidence` and `bundle` already have established subcommand surfaces.
- `replay` and `import` may appear in scripts.
- The benefit is real but less urgent than MCP/trust grouping.

Trigger to start:

- A future evidence importer or replay UX pass.
- Repeated confusion between bundles, receipts, replay, and imports.

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

1. Land this RFC as docs-only.
2. Wait for a concrete reason to touch MCP command code, or schedule a small
   MCP-only grouping PR.
3. If MCP grouping lands cleanly, consider a trust grouping PR.
4. Defer policy/evidence grouping until there is user feedback or nearby
   maintenance work.
5. Do not group core commands.

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
