# Claude Plugin Marketplace Packaging Design

Date: 2026-08-09
Issue: [#2152](https://github.com/Rul1an/assay/issues/2152), step 4
Base: `48beb09d1ba581f275da7775228262c778914a28`

## Goal

Publish the already shipped `assay-mcp-server` and generated
`assay-golden-path` journey as one installable Claude Code marketplace plugin.
The installed plugin must work from Claude Code's isolated plugin cache without
reading the Assay source checkout, while its MCP server evaluates policy from
the consuming project.

This is packaging, not a new enforcement or evidence capability.

## Measured Constraints

The design is based on direct probes with Claude Code `2.1.32` and the server
built from the base SHA above:

- Claude Code copies installed plugins into an isolated cache. Paths outside
  the plugin root are unavailable after installation.
- `${CLAUDE_PLUGIN_ROOT}` resolves in skill content. `${CLAUDE_PROJECT_DIR}` is
  accepted in stdio MCP arguments and substituted when the variable is present
  in the environment. The standalone health subcommand does not synthesize it;
  session-time synthesis remains unproven until the Task 3 session check.
- Sixteen inspected plugin artifacts are mixed: six use an `mcpServers` wrapper
  and ten use a bare server map. The installed cache artifact uses the wrapper;
  the bare examples are predominantly marketplace-source artifacts. Neither
  shape alone proves what the installed client executes. This design selects
  the installed partner plugin's wrapper, but acceptance requires a real-client
  connection probe plus the full manifest-driven `initialize` and `tools/list`
  exchange below.
- A plugin with no explicit version installs under a SHA-derived version and
  advances after a marketplace update.
- `claude plugin validate` accepts a referenced `.mcp.json` whose `command` is
  a number, so successful vendor validation is insufficient by itself.
- A default-feature `initialize` plus `tools/list` exchange returns exactly the
  five production tools and no `assay_test_outbound`.
- The compact `tools/list` JSON-RPC response is 8,120 UTF-8 bytes. This is a
  wire-byte measurement, not a token, per-turn, per-session, or cache claim.

The current project skill is not directly packageable. It instructs callers to
read `docs/generated/agent-golden-path.json`, edit
`scripts/docs/generate-agent-golden-path.py`, and run step 6 from
`examples/privileged-action-gate`. Those paths do not exist in an installed
plugin cache.

## Package Layout

The repository publishes one marketplace containing one plugin made of MCP
configuration and three fixtures, one of which is executable Python:

```text
.claude-plugin/
  marketplace.json
packaging/claude-plugin/
  .claude-plugin/
    plugin.json
  .mcp.json
  skills/
    assay-golden-path/
      SKILL.md
      references/
        agent-golden-path.json
      assets/
        privileged-action-gate/
          mock_github_mcp.py
          baseline-approved.json
          policies/
            no-allowance.yaml
```

The marketplace and plugin are both named `assay`, giving install identity
`assay@assay`. Claude Code auto-discovers
`skills/assay-golden-path/SKILL.md`; `plugin.json` does not invent an unsupported
`skills` field. The package has no commands or agents.

The marketplace manifest has the explicit minimal field set `name`, `owner`,
and `plugins`; `owner` is `{ "name": "Assay" }`. Its sole plugin entry has
`name`, `description`, and local `source: "./packaging/claude-plugin"`. The
plugin manifest has `name`, `description`, and
`author: { "name": "Assay" }`; neither manifest has a `version`. These fields
follow inspected local-source marketplace artifacts rather than relying on
directory discovery for the marketplace entry itself.

The plugin omits `version`. Packaging-only changes must not be coupled to the Rust
crate release cadence, and the measured Claude client already provides a
commit-derived version for an unversioned git marketplace. The absence is a
tested decision rather than an omitted field by accident.

## MCP Configuration

The plugin's `.mcp.json` contains one stdio server under `mcpServers.assay`:

```json
{
  "mcpServers": {
    "assay": {
      "command": "assay-mcp-server",
      "args": ["--policy-root", "${CLAUDE_PROJECT_DIR}"]
    }
  }
}
```

The plugin does not bundle `assay-mcp-server`; that binary must already be on
`PATH`. The server reads policies from the consuming project, not the
marketplace checkout or plugin cache. The manifest has no unverified `note`
extension field; limits belong in the generated skill and install guide.

## Generated Skill Profile

`scripts/docs/generate-agent-golden-path.py` remains the single owner of the
golden-path contract and all three skill renderings:

1. `.agents/skills/assay-golden-path/SKILL.md` for project discovery;
2. `.claude/skills/assay-golden-path/SKILL.md` for project discovery;
3. `packaging/claude-plugin/skills/assay-golden-path/SKILL.md` for an installed
   plugin.

The two project skills remain byte-identical. The plugin profile intentionally
differs only where location semantics differ:

- its authoritative contract is
  `${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/references/agent-golden-path.json`;
- it does not tell a user to edit a generator absent from the plugin;
- step 6 assets resolve under
  `${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/assets/privileged-action-gate`.

The bundled JSON remains byte-identical to
`docs/generated/agent-golden-path.json`. Its schema-v1 `working_directory`
continues to mean a path relative to the source repository. The plugin profile
does not rewrite that field and does not add `working_directory_base`.
Instead, it includes one generated declarative mapping sentence:

> Fixtures named by the contract under `examples/privileged-action-gate` are
> bundled at
> `${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/assets/privileged-action-gate`.

That sentence translates packaging location without creating a second Assay
contract vocabulary. Static checks reject executable or read instructions that
resolve through repository-only paths, while allowing this one named mapping.

## Bundled Resources

Only the files required by the step-6 command are bundled:

- `mock_github_mcp.py`;
- `baseline-approved.json`;
- `policies/no-allowance.yaml`.

The Python fixture imports only the standard library and opens no additional
local files. The JSON and YAML fixtures do not reference other files. The
remaining files in `examples/privileged-action-gate` serve other demos and are
excluded.

This is not a config-only package: `mock_github_mcp.py` is executable fixture
code that step 6 starts through the user's Python interpreter. The install
guide therefore requires `python3` or `python` on `PATH` for that optional
step, separately from the mandatory `assay-mcp-server` prerequisite.

The generator copies these three resources byte-for-byte. The generated-docs
drift gate covers their package destinations, so source changes cannot leave a
stale plugin copy. The package does not fetch mutable GitHub URLs and does not
require an Assay checkout.

## Installation Documentation

`docs/guides/editor-mcp-recipe.md` adds the measured Claude Code path:

1. install `assay-mcp-server` from a reviewed release or checkout;
2. add the repository marketplace;
3. install `assay@assay`;
4. verify the plugin and its MCP server from the consuming project;
5. after a marketplace update, run the plugin update and inspect the installed
   cache version so stale cache state is visible rather than inferred away.

The guide says plainly that the plugin does not bundle the binary, does not
enforce anything merely by being installed, and consumes policy from the
current Claude project.

## Verification Design

### Static and generation gates

Extend the existing golden-path validator so it proves:

- the generator owns all three skill destinations;
- the two project skills remain byte-identical;
- the plugin profile contains only `${CLAUDE_PLUGIN_ROOT}` resource
  instructions plus the one explicit source-to-bundle mapping;
- the bundled contract and three fixtures are byte-identical to their canonical
  sources;
- generated drift catches edits to every packaged generated resource;
- private planning vocabulary and unsupported claims remain absent.

Parse, do not search, the marketplace and plugin JSON. Assert the install
identity, exact owner and author field shapes, local source path,
auto-discovered skill location, `mcpServers` wrapper, string command, exact
argument vector, and absence of `version`.

### Driven server contract

A workspace-only `assay-mcp-server` integration test reads the plugin MCP entry,
uses its argument vector with `${CLAUDE_PROJECT_DIR}` replaced by the temporary
consuming-project directory, and launches
`CARGO_BIN_EXE_assay-mcp-server`. An `initialize` plus `tools/list` exchange must
return these names exactly:

- `assay_check_args`;
- `assay_check_coverage`;
- `assay_check_sequence`;
- `assay_explain_trace`;
- `assay_policy_decide`.

The default build must omit `assay_test_outbound`; an all-features run retains
the existing bidirectional feature assertion.

### Real-client proof

Before push, run both layers with the installed Claude client:

- `claude plugin validate` on the marketplace and plugin;
- isolated marketplace add/install/update using a fresh
  `CLAUDE_CONFIG_DIR`;
- inspect the cached plugin rather than the source directory;
- from a disposable consuming project, run
  `CLAUDE_PROJECT_DIR="$PWD" claude mcp list`; the standalone health subcommand
  does not synthesize this session variable, so the explicit value proves that
  the cached wrapper-shaped configuration is accepted and the stdio server
  connects, but does not prove session-time variable injection;
- drive the cached `.mcp.json` against the built server through a complete
  `initialize` plus `tools/list` exchange, not merely a config-listing command;
- invoke an actual Claude Code session from a disposable project without
  manually setting `CLAUDE_PROJECT_DIR`, and verify both session-time MCP
  connection and discovery of the packaged `assay-golden-path` skill.

Record the exact client version, source SHA, installed cache version, commands,
and outcomes in the PR review packet. Vendor validation supplements rather than
replaces repository assertions.

These real-client steps remain a manual pre-push procedure. If they are later
automated through `tests/support/bounded_process.rs`, that automation must use
the process-tree and early-EPIPE semantics accepted by #2189; marketplace
operations may spawn `git`, so direct-child-only timeout handling is not an
equivalent proof.

### TDD and mutation proof

Write static and runtime tests before package files exist and confirm they fail
on the named missing artifact, not merely on compile setup. After green, each
of these temporary mutations must fail its named assertion:

| mutation | required failure |
|---|---|
| set plugin MCP `command` to `42` | typed MCP command assertion |
| alter one bundled fixture byte | canonical resource parity assertion |
| point a packaged read/run instruction at `docs/`, `scripts/`, or source `examples/` | cache-safe instruction assertion |
| rename one production tool | exact release tool-surface assertion |
| add plugin or marketplace `version` | unversioned git-marketplace decision assertion |
| remove the source-to-bundle mapping sentence | packaged contract-location assertion |

Every mutation is restored before final verification.

## Security and Failure Policy

- Treat marketplace, manifest, skill, and bundled fixture inputs as hostile in
  tests; bound reads before materialization.
- Manifest/schema/type errors fail closed.
- Missing `assay-mcp-server` is reported as a host spawn failure; the plugin
  must not imply that a server ran.
- Missing Python blocks only the optional step-6 fixture workflow and is named
  as a prerequisite failure; it is not reported as an Assay policy verdict.
- Missing project policy never becomes a clean enforcement result.
- The bundled mock is a local stdio fixture. It performs no network or provider
  action.
- No credentials, OAuth, network endpoint, or persistent access are introduced.

## Non-Goals

- No self-contained cross-platform binary distribution.
- No new MCP tool, result envelope, cache field, or protocol-version claim;
  [#2157](https://github.com/Rul1an/assay/issues/2157) owns those changes.
- No `tools/list` token, per-turn, per-session, or host-cache claim;
  [#2185](https://github.com/Rul1an/assay/issues/2185) owns wire measurement.
- No Cursor plugin-runtime claim.
- No proof of provider execution or external side effects.
- No trust score, whole-action verdict, compliance claim, or certification.
