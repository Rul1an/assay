# Claude Plugin Marketplace Packaging Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the existing `assay-mcp-server` and generated `assay-golden-path` journey as an installable, cache-safe Claude Code marketplace plugin.

**Architecture:** Keep the golden-path Python generator as the single owner of the contract, all three skill renderings, and packaged fixtures. Add parsed static contracts for marketplace/plugin identity and a Rust integration test that drives the manifest arguments against the workspace server binary. Treat vendor validation and an isolated installed-client probe as additional evidence, never as a substitute for repository assertions.

**Tech Stack:** Python 3 standard library, Bash, JSON manifests, Rust integration tests, Claude Code CLI with its exact measured version recorded during proof.

## Global Constraints

- Work only in the dedicated worktree on `codex/2152-plugin-packaging`; use a private `CARGO_TARGET_DIR`.
- Implement against approved design commit `ed6accbb0fe5c44a810cf6414d7afdbefffff8e8`.
- Plugin identity is `assay@assay`; neither marketplace nor plugin manifest has a `version`.
- MCP is `assay-mcp-server --policy-root .`; no binary is bundled.
- Default server exposes exactly five named production tools; `assay_test_outbound` is feature-only.
- Package exactly three canonical fixtures, including executable Python; Python is optional except for journey step 6.
- Bound hostile manifest/resource reads to 1 MiB and reject symlinks or invalid types.
- Do not claim token, per-turn, per-session, host-cache, provider execution, external side effects, compliance, certification, or Cursor plugin-runtime behavior.
- Use exact pathspec staging. A new push invalidates prior reviews and runtime proof.

---

### Task 0: Falsify the Selected Manifest Shape Before Committing It

**Files:**
- No repository files; use only a disposable directory and fresh `CLAUDE_CONFIG_DIR`.

**Interfaces:**
- Consumes: installed Claude CLI, the approved wrapper-shaped MCP entry, and `assay-mcp-server` on an isolated `PATH`.
- Produces: exact client-version/argv/output evidence selecting wrapper or bare server-map shape for Task 1.

- [x] **Step 1: Build an ephemeral minimal marketplace and plugin**

Create the approved marketplace/plugin metadata and wrapper-shaped `.mcp.json` under `mktemp -d`; do not copy these files into the worktree. Use the installed client's help output to derive marketplace add/install/list syntax.

- [x] **Step 2: Drive the installed client from a disposable project**

With a fresh `CLAUDE_CONFIG_DIR`, add the temporary marketplace, install `assay@assay` at local scope, and inspect the installed cache rather than the source directory. Run `CLAUDE_PROJECT_DIR="$PWD" claude mcp list`: the standalone health subcommand does not synthesize this session variable, so this proves cached-wrapper parsing and connection but not session-time injection. Then drive the cached manifest against the built server through full `initialize` and `tools/list`.

- [x] **Step 3: Branch explicitly on the result**

If the wrapper connects and returns the five exact tools, record it and continue. If the wrapper is rejected, repeat once with a bare server map. Update the approved design and obtain a new design review before Task 1 if and only if the bare shape succeeds. If neither works, stop as a client/plugin compatibility blocker; do not commit either shape.

- [x] **Step 4: Record provenance**

Record exact source SHA, `claude --version`, generated temporary paths, executed argv, installed cache version/path, `mcp list`, `initialize`, and `tools/list` outcomes. This preflight selects a shape; Task 3 later proves the complete shipped package and update workflow.

**Task 0 evidence:** At source SHA
`932ea9507560e47b0dbd3a3840218b8da601e2a8`, Claude Code `2.1.32`
accepted the wrapper, installed `assay@assay` at local scope into its isolated
cache, and exposed the entry through `plugin list --json`. With
`CLAUDE_PROJECT_DIR` explicitly set to the disposable project, `claude mcp
list` reported `Connected`. The cached manifest independently completed
`initialize` and `tools/list` and returned the five exact production names.
Without the explicit variable, the standalone health subcommand failed before
spawn and its debug log reported `Missing environment variables in plugin MCP
config: CLAUDE_PROJECT_DIR`. Task 3 then falsified session-time injection too:
Claude Code `2.1.32` passed the literal placeholder to the server. A disposable
`--policy-root .` control connected from the consuming project and discovered
the packaged skill, so the implementation now pins `.` and separately proves
the root with a consumer-only policy. All temporary config, marketplace, cache,
and project paths were deleted after each completed probe.

### Task 1: Pin Marketplace, Plugin, and Driven MCP Contracts

**Files:**
- Modify: `scripts/ci/test-agent-golden-path-skill.py`
- Modify: `crates/assay-mcp-server/tests/project_install_surfaces.rs`
- Create after RED: `.claude-plugin/marketplace.json`
- Create after RED: `packaging/claude-plugin/.claude-plugin/plugin.json`
- Create after RED: `packaging/claude-plugin/.mcp.json`
- Modify after RED: `.gitattributes`

**Interfaces:**
- Consumes: `read_bounded_evidence`, the existing bounded install-surface reader,
  `CARGO_BIN_EXE_assay-mcp-server`, and JSON-RPC `Conn`.
- Produces: exact parsed manifest contracts, `InstallFile::PluginManifest`, and one shared
  manifest-to-release-surface driver for project and plugin installs.

- [x] **Step 1: Write failing parsed-manifest assertions**

Add bounded paths and compare parsed objects, not text:

```python
MARKETPLACE_PATH = ROOT / ".claude-plugin/marketplace.json"
PLUGIN_MANIFEST_PATH = ROOT / "packaging/claude-plugin/.claude-plugin/plugin.json"
PLUGIN_MCP_PATH = ROOT / "packaging/claude-plugin/.mcp.json"

marketplace = json.loads(read_bounded_evidence(MARKETPLACE_PATH, "marketplace manifest"))
if marketplace != {
    "name": "assay",
    "owner": {"name": "Assay"},
    "plugins": [{
        "name": "assay",
        "description": EXPECTED_PLUGIN_DESCRIPTION,
        "source": "./packaging/claude-plugin",
    }],
}:
    fail("Claude marketplace identity or local source drifted")
```

Assert exact plugin fields (`name`, `description`, `author`), absence of `version`, and one `mcpServers.assay` entry with string command plus exact `--policy-root .` args. Assert that the project and plugin entries intentionally match.

- [x] **Step 2: Write the failing manifest-driven Rust test**

```rust
#[test]
fn plugin_manifest_drives_the_release_server_surface() {
    let entry = manifest_entry(InstallFile::PluginManifest);
    assert_eq!(entry["command"], "assay-mcp-server");
    let project = tempfile::tempdir().expect("temporary Claude project");
    let args = plugin_args(&entry, project.path());
    let mut connection = Conn::attach(spawn_server(project.path(), &args));
    assert!(connection.request("initialize", initialize_params(), 1)["result"].is_object());
    let tools = connection.request("tools/list", serde_json::json!({}), 2);
    assert_eq!(release_tool_names(&tools), EXPECTED_RELEASE_TOOLS);
}
```

Pin the five names, not only the count, and retain bidirectional `test-outbound` coverage.

- [x] **Step 3: Verify RED**

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
CARGO_TARGET_DIR=/tmp/assay-2152-target cargo test -p assay-mcp-server --test project_install_surfaces plugin_manifest_drives_the_release_server_surface -- --exact --nocapture
```

Both must fail on the named missing manifest, not on compilation or setup.

- [x] **Step 4: Create only the minimal typed manifests**

Create the exact approved JSON. Add LF attributes. Do not add versions, commands, agents, auth, network, or a `note` extension.

- [x] **Step 5: Verify GREEN**

Run both Step 3 commands, the full focused test target, and its `--features test-outbound` variant. Default must return the five exact production names; all-features may add only `assay_test_outbound`.

- [x] **Step 6: Commit**

```bash
git add -A -- scripts/ci/test-agent-golden-path-skill.py crates/assay-mcp-server/tests/project_install_surfaces.rs .claude-plugin/marketplace.json packaging/claude-plugin/.claude-plugin/plugin.json packaging/claude-plugin/.mcp.json .gitattributes
git commit -m "feat(plugin): pin Claude marketplace and MCP contracts"
```

### Task 2: Generate the Cache-Safe Skill and Canonical Resources

**Files:**
- Modify: `scripts/docs/generate-agent-golden-path.py`
- Modify: `scripts/ci/test-agent-golden-path-skill.py`
- Modify: `scripts/ci/check-docs-generated-drift.sh`
- Modify: `scripts/ci/test-agent-golden-path-skill-hardening.sh`
- Modify: `.pre-commit-config.yaml`
- Modify: `.github/workflows/kernel-matrix.yml` so `packaging/claude-plugin/**`
  and `.claude-plugin/**` changes trigger the head-side contract gates instead
  of bypassing them through the workflow path filter.
- Generate: `packaging/claude-plugin/skills/assay-golden-path/{SKILL.md,references/agent-golden-path.json,assets/privileged-action-gate/**}`

**Interfaces:**
- Consumes: `CONTRACT`, project skill renderer, canonical step-6 fixtures.
- Produces: `render_plugin_skill() -> str` and `PLUGIN_RESOURCE_COPIES: tuple[tuple[Path, Path], ...]`.

- [x] **Step 1: Write failing profile/resource checks**

Require plugin-root contract guidance, the exact source-to-bundle mapping, no executable/read instruction through source-only `docs/`, `scripts/`, or `examples/`, and byte parity:

```python
for source, packaged in PLUGIN_RESOURCE_COPIES:
    if read_bounded_evidence(source, "canonical resource") != read_bounded_evidence(
        packaged, "packaged resource"
    ):
        fail(f"packaged resource drifted: {packaged.relative_to(ROOT)}")
```

- [x] **Step 2: Extend drift/hardening selectors and verify RED**

Add all generated package files to `GENERATED` and wholly recreated outputs to `FRESH_GENERATED`. Seed them in hardening scratch repos. Add named failures for missing mapping, source-only instructions, and fixture-byte drift. Run validator, drift, and hardening scripts; failures must name absent generated outputs.

- [x] **Step 3: Implement one shared renderer with a plugin profile**

Extract only the shared journey rendering. Keep project skills byte-identical. The plugin profile uses `${CLAUDE_PLUGIN_ROOT}/skills/assay-golden-path/references/agent-golden-path.json`, omits source-generator editing instructions, and adds exactly one approved fixture mapping sentence.

- [x] **Step 4: Copy canonical resources in the generator**

Use `shutil.copyfile` after creating parents. Copy generated contract bytes after writing them and only the three approved fixtures: `mock_github_mcp.py`, `baseline-approved.json`, and `policies/no-allowance.yaml`.

- [x] **Step 5: Generate and verify GREEN**

```bash
python3 scripts/docs/generate-agent-golden-path.py
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/check-docs-generated-drift.sh
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
cmp .agents/skills/assay-golden-path/SKILL.md .claude/skills/assay-golden-path/SKILL.md
```

- [x] **Step 6: Commit exact generator/gate/output paths**

Commit as `feat(plugin): generate cache-safe golden-path resources`.

### Task 3: Document and Drive the Installed Claude Workflow

**Files:**
- Modify: `docs/guides/editor-mcp-recipe.md`
- Create: `scripts/ci/test-claude-plugin-install.sh`
- Create: `scripts/ci/claude_plugin_install_workflow.py`
- Modify: `.pre-commit-config.yaml`

**Interfaces:**
- Consumes: marketplace root, `assay@assay`, server on `PATH`, Claude CLI.
- Produces: bounded disposable-client proof recording client version, source SHA, cache version, health-subcommand connection, actual-session connection, consumer-project policy-root proof, and each phase.

- [x] **Step 1: Write a failing workflow self-test**

Use a fake Claude executable and require validate, add/install/update, cache inspection, `mcp list`, complete JSON-RPC exchange, actual-session MCP connection, consumer-only policy-root proof, and skill discovery. It must reject a source-directory-only success.

- [x] **Step 2: Verify RED**

Run `bash scripts/ci/test-claude-plugin-install.sh --self-test`; expect the named missing update/cache/protocol phase.

- [x] **Step 3: Add honest installation documentation**

State that the plugin bundles no server, installation alone enforces nothing, policy resolves from the Claude project, Python is optional except for step 6, spawn failure is not an Assay verdict, and update plus cache inspection exposes stale state.

- [x] **Step 4: Implement the bounded disposable workflow**

Use fresh `CLAUDE_CONFIG_DIR`, temporary marketplace and consuming project, trap cleanup, 1 MiB output ceilings, and process-tree deadlines equivalent to #2189. Do not inject `CLAUDE_PROJECT_DIR`. Prove both health-subcommand and actual-session MCP connection, then prove policy-root resolution with a policy available only inside the consuming project. Never mutate normal Claude config or pass credentials. Fail if installed cache resolves inside the source checkout.

The implementation uses a thin shell entrypoint and a standard-library Python
driver because the existing Rust helper is integration-test-only and cannot
bound vendor CLI commands without compiling a test harness. The scoped driver
starts a POSIX session, kills the process group, keeps one absolute deadline
while draining inherited pipes, and caps stdout/stderr separately. Its self-test
includes hangs, a direct child that exits while a descendant retains the pipe,
and output overflow. This is a macOS/Linux workflow proof, not a Windows-native
process-supervision claim.

The consumer probe has no `policies/` directory and pins the installed argv to
`--policy-root .` before writing a unique root-level policy. That prevents the
server's default `policies` value from making the policy denial false-green. A
fake update that exits zero but retains stale cache bytes must fail on cache
parity. Failures use stable `phase`, `status`, `reason`, and `next_step` lines;
success evidence uses one scan-friendly `key=pass` line per proven boundary.

- [x] **Step 5: Run the real-client proof**

Build/install the exact-SHA server into an isolated bin directory and record:

```text
source_sha=<40 hex>
claude_version=<exact output>
installed_cache_version=<exact value>
plugin_validate=pass
mcp_list_connected=pass
actual_session_mcp_connected=pass
policy_root_resolved_to_consumer=pass
missing_policy_refused=pass
initialize=pass
tools_list=pass
skill_discovery=pass
model_mediated_tool_call=unavailable
verification=pass
```

The last line means the scoped installation/protocol proof passed. It does not
promote `model_mediated_tool_call=unavailable` to a model/tool-use pass. If the
fresh client unexpectedly has working authentication, the status is
`not_exercised` unless a separate test actually observes a model-mediated tool
call. Unavailable vendor behavior is unavailable/failed proof, never pass.

- [x] **Step 6: Commit**

Stage the exact guide, workflow script, and hook paths. Commit as `docs(plugin): add measured Claude install workflow`.

**Task 3 evidence:** Commit
`c61b43959b67c62037c3403c871d627bea517375` built
`assay-mcp-server 5.0.0` with rustc/cargo `1.96.0` and drove Claude Code
`2.1.32` from a fresh config and consumer project. Marketplace validation,
add/install/update, cache parity, MCP health, `initialize`, the five exact tools,
consumer-root policy denial, actual-session MCP startup, and packaged-skill
discovery passed. Installed cache version was `c61b43959b67`.
`model_mediated_tool_call=unavailable` because the disposable client had no
credentials; no model/tool-use or provider-execution claim is made. The final
head re-runs this proof after this evidence note is committed.

### Task 4: Mutation, Full Verification, and PR

**Files:**
- Modify if needed: `scripts/ci/test-agent-golden-path-skill-hardening.sh`
- Modify if needed: `scripts/ci/test-claude-plugin-install.sh`
- Update: this plan's checkboxes and evidence notes.

- [x] **Step 1: Run six required mutations individually**

Each temporary mutation must reach its own guard, then be restored: command becomes number; fixture byte changes; instruction points to source-only path; production tool is renamed; manifest version is added; mapping sentence is removed.

- [x] **Step 2: Run final verification**

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-optimization.sh
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
bash scripts/ci/check-docs-generated-drift.sh
CARGO_TARGET_DIR=/tmp/assay-2152-target cargo test -p assay-mcp-server --test project_install_surfaces -- --nocapture
CARGO_TARGET_DIR=/tmp/assay-2152-target cargo test -p assay-mcp-server --features test-outbound --test project_install_surfaces -- --nocapture
cargo fmt --all -- --check
CARGO_TARGET_DIR=/tmp/assay-2152-target cargo clippy -p assay-mcp-server --all-targets --all-features -- -D warnings
git diff --check
```

- [x] **Step 3: Run user/workflow/AI simulations**

Exercise missing binary, missing optional Python, missing policy, stale cache before update, successful update/cache inspection, connected five-tool surface, skill discovery, and protected-action denial. Verify host/prerequisite/policy failures remain distinct and absence never becomes clean.

**Task 4 evidence before PR:** All six required mutations bit their named
guards: typed MCP command, fixture parity, source-only instruction, exact server
tool names, unversioned plugin identity, and fixture mapping. The generated-skill
hardening suite observed 84 cases plus 22 structural probes. The disposable
workflow additionally rejected a missing server at `phase=prerequisite`, a
source-checkout cache, a successful no-op update retaining stale bytes, tool
drift, output overflow, a hung process tree, and an exited parent whose
descendant retained the output pipe. The exact built server denied the
consumer-only marker and returned `E_POLICY_NOT_FOUND` for a missing policy;
neither absence became clean. The generated contract contains `<python>` only
in protected-action step 6; installation and steps 1-5 do not claim Python is
needed. A fresh unauthenticated Claude session connected the MCP server and
discovered the skill while leaving model-mediated tool use unavailable.

On head `a43edcb7e927894c1973635d71be0ce56b73ced8`, the full
`assay-mcp-server` crate suite, the `test-outbound` install surface, all-targets
and all-features clippy with `-D warnings`, fmt, and diff checks passed. The
first post-mutation client run correctly caught that the debug binary still
contained the temporary renamed tool even though source was restored. Rebuilding
from the committed head restored artifact provenance, after which all installed
workflow phases, including `missing_policy_refused=pass`, passed.

- [ ] **Step 4: Push and open an audit-grade PR**

Include exact SHA/worktree, diffstat, hot files, contract changes, RED/GREEN evidence, six mutation failures, real-client outputs, threat-model delta, fail-open/closed table, compatibility, and non-claims.

- [ ] **Step 5: Obtain exact-head quorum and merge only when green**

Follow the repository `AGENTS.md` quorum: one non-building agent review on the
final head. Automated reviews can add evidence but neither satisfy nor block
quorum. Fix or technically disposition every actionable finding. Any push
restarts review and proof.

## Self-Review

- Spec coverage: pre-commit shape selection, identity, manifests, generated package, canonical resources, installed client, drift, mutation, security/failure policy, and non-claims each map to a task.
- Placeholder scan: no deferred implementation or unnamed error-handling instruction remains.
- Type consistency: one profiled `render_skill` function and `PLUGIN_RESOURCE_COPIES` are consumed consistently; the existing install-surface driver owns the MCP invocation and consumer-policy-root rules.
- Residual: actual-session proof can establish MCP and skill startup without an authenticated API call; model-mediated tool use remains unavailable in a fresh unauthenticated `CLAUDE_CONFIG_DIR` and must not be reported as pass.
