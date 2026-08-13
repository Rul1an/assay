# Outward Product Truth Slice 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Assay's active install, quickstart, CLI, plugin, skill, and MCP documentation agree with release `v5.1.0` and fail CI when those claims drift again.

**Architecture:** Extend the existing release-surface checker rather than creating a second version authority. Keep `docs/generated/agent-golden-path.json` and its generator as the machine-facing owner, add release metadata there, and make human quickstarts link to the generated canonical journey instead of copying command sequences.

**Tech Stack:** Bash, Python 3 standard library, MkDocs, pre-commit, generated Markdown/JSON, Rust CLI contract tests.

## Global Constraints

- Released claims are measured against `v5.1.0`; source-only behavior is labelled `Unreleased`.
- Use only verified channels: GitHub releases, `getassay.dev/install.sh`, crates.io, PyPI `assay-it`, GitHub Marketplace, MCP Registry, and the repository Claude plugin marketplace.
- Do not advertise Homebrew, Scoop, or GHCR until those channels are published and measured.
- Do not describe `assay init --from-trace` output as an MCP authorization policy.
- Keep `.mcp.json` and `.cursor/mcp.json` byte-identical and do not edit their invocation.
- Do not add a hand-maintained capability matrix; issue #1977 owns that future artifact.
- All edits occur in a dedicated `codex/` worktree; stage only named paths.

---

### Task 1: Prepare release-surface mutation coverage RED

**Files:**
- Create: `scripts/ci/test-check-release-surface.sh`
- Modify: `scripts/ci/check-release-surface.sh`
- Modify: `.pre-commit-config.yaml`

**Interfaces:**
- Consumes: `[workspace.package].version`, the existing `ASSAY_BIN` override, and the active-doc path list below.
- Produces: one `release-surface` hook that checks version alignment and rejects unsupported active installation claims.

- [ ] **Step 1: Write the failing self-test**

Create a shell test that copies only the checker inputs into a temporary Git repository, supplies a fake `assay` binary that prints `assay 5.1.0`, and runs these mutations independently:

```bash
mutate_and_expect_failure \
  wrong-python-package \
  docs/getting-started/index.md \
  's/pip install assay-it/pip install assay/' \
  'unsupported Python package'

mutate_and_expect_failure \
  homebrew-channel \
  docs/getting-started/installation.md \
  '/## Quick Install/a\brew install rul1an/tap/assay' \
  'unsupported Homebrew channel'

mutate_and_expect_failure \
  scoop-channel \
  docs/getting-started/installation.md \
  '/### Windows/a\scoop bucket add assay https://github.com/Rul1an/scoop-assay' \
  'unsupported Scoop channel'

mutate_and_expect_failure \
  ghcr-channel \
  docs/getting-started/installation.md \
  '/### Windows/a\docker pull ghcr.io/rul1an/assay:latest' \
  'unsupported GHCR image'

mutate_and_expect_failure \
  stale-windows-asset \
  docs/getting-started/installation.md \
  's/assay-v5.1.0-x86_64-pc-windows-msvc.zip/assay-windows-x86_64.zip/' \
  'obsolete Windows asset name'
```

The helper must assert a non-zero exit and grep the named diagnostic. It must also run the unmodified case and require exit zero.

- [ ] **Step 2: Run the self-test and confirm RED**

Run:

```bash
bash scripts/ci/test-check-release-surface.sh
```

Expected: FAIL because `check-release-surface.sh` does not yet reject at least one injected claim.

- [ ] **Step 3: Extend the existing checker without committing it yet**

Add a derived active-doc check after the documented CLI version check. Keep the rule in this single checker:

```bash
check_absent() {
  file="$1"
  literal="$2"
  label="$3"
  if grep -Fq "$literal" "$file"; then
    fail "$file: $label: $literal"
  fi
}

check_absent docs/getting-started/index.md 'pip install assay' 'unsupported Python package'
check_absent docs/getting-started/installation.md 'brew install rul1an/tap/assay' 'unsupported Homebrew channel'
check_absent docs/getting-started/installation.md 'Rul1an/scoop-assay' 'unsupported Scoop channel'
check_absent docs/getting-started/installation.md 'ghcr.io/rul1an/assay' 'unsupported GHCR image'
check_absent docs/getting-started/installation.md 'assay-windows-x86_64.zip' 'obsolete Windows asset name'
```

Use exact literals, not broad words such as `brew`, `docker`, or `assay`, because historical and troubleshooting prose legitimately contains them.

Change the pre-commit hook entry to:

```yaml
entry: bash -c 'bash scripts/ci/test-check-release-surface.sh && bash scripts/ci/check-release-surface.sh'
```

Expand its `files:` expression to include the new self-test and the active docs checked by the script.

- [ ] **Step 4: Run the checker and mutation suite**

Run:

```bash
bash scripts/ci/test-check-release-surface.sh
ASSAY_BIN=target/debug/assay bash scripts/ci/check-release-surface.sh
```

Expected: the self-test reports all six mutations observed; the live checker still fails until Task 3 removes existing unsupported claims. Do not commit this intermediate red repository state. Keep these three paths owned by the same writer through Task 3.

### Task 2: Make the generated journey release-pinned

**Files:**
- Modify: `scripts/docs/generate-agent-golden-path.py`
- Modify: `scripts/ci/test-agent-golden-path-skill.py`
- Modify: `docs/generated/agent-golden-path.json`
- Modify: `docs/guides/agent-golden-path.md`
- Modify: `.agents/skills/assay-golden-path/SKILL.md`
- Modify: `.claude/skills/assay-golden-path/SKILL.md`
- Modify: `packaging/claude-plugin/skills/assay-golden-path/SKILL.md`
- Modify: `packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json`

**Interfaces:**
- Consumes: `[workspace.package].version` from `Cargo.toml` and the existing nine-step `STEPS` table.
- Produces: `release_version` and `release_tag` in the machine contract plus one generated release-pinned start block in the guide.

- [ ] **Step 1: Add failing release-metadata assertions**

In `scripts/ci/test-agent-golden-path-skill.py`, load the workspace version with `tomllib` and assert:

```python
workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
version = workspace["workspace"]["package"]["version"]
require_equal(contract.get("release_version"), version, "golden-path release_version")
require_equal(contract.get("release_tag"), f"v{version}", "golden-path release_tag")
```

Also require the guide to contain exactly one pair of:

```text
<!-- agent-golden-path-release:start -->
<!-- agent-golden-path-release:end -->
```

and to name `assay {version}`, `assay version`, the GitHub release tag, upgrade, rollback, and the release-vs-main distinction.

- [ ] **Step 2: Run the contract and confirm RED**

Run:

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
```

Expected: FAIL because `release_version` is absent.

- [ ] **Step 3: Generate release metadata and the human start block**

Use `tomllib` in the generator:

```python
WORKSPACE = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
RELEASE_VERSION = WORKSPACE["workspace"]["package"]["version"]
RELEASE_TAG = f"v{RELEASE_VERSION}"
```

Add both values to `CONTRACT`. Add a generated guide block with exact semantics:

```markdown
## Release-pinned start

This journey is pinned to Assay `5.1.0` (`v5.1.0`). Install the CLI from a
verified channel, then require `assay version` to print `5.1.0` before using
the table below. Behavior merged after the tag is `Unreleased` and is not part
of this release claim.

Upgrade by installing the newer explicit release and re-running all nine
steps. Roll back by reinstalling `v5.1.0` from the GitHub release assets and
re-running the same journey.
```

Render the actual values from `Cargo.toml`; do not hard-code `5.1.0` in Python. Keep skill bodies focused on operation and do not duplicate the install section there.

- [ ] **Step 4: Regenerate and verify all owned files**

Run:

```bash
python3 scripts/docs/generate-agent-golden-path.py
python3 scripts/docs/generate-agent-golden-path.py --check
python3 scripts/ci/test-agent-golden-path-skill.py
cargo test -p assay-cli --test agent_golden_path_contract
cargo test -p assay-mcp-server --test agent_golden_path_contract
```

Expected: all pass and generated files are byte-stable on the second generator run.

- [ ] **Step 5: Commit generated ownership together**

```bash
git add -- scripts/docs/generate-agent-golden-path.py scripts/ci/test-agent-golden-path-skill.py docs/generated/agent-golden-path.json docs/guides/agent-golden-path.md .agents/skills/assay-golden-path/SKILL.md .claude/skills/assay-golden-path/SKILL.md packaging/claude-plugin/skills/assay-golden-path/SKILL.md packaging/claude-plugin/skills/assay-golden-path/references/agent-golden-path.json
git commit -m "docs(golden-path): pin journey to workspace release"
```

### Task 3: Reconcile installation and quickstart entrypoints

**Files:**
- Modify: `README.md`
- Modify: `docs/index.md`
- Modify: `docs/getting-started/index.md`
- Modify: `docs/getting-started/installation.md`
- Modify: `docs/getting-started/quickstart.md`
- Modify: `docs/getting-started/ci-integration.md`
- Modify: `docs/reference/cli/index.md`
- Modify: `docs/python-sdk/index.md`
- Modify: `docs/use-cases/air-gapped.md`
- Modify: `docs/AIcontext/user-flows.md`
- Modify: `docs/migration-v1.2.md`

**Interfaces:**
- Consumes: the verified release channels and generated journey from Tasks 1 and 2.
- Produces: short outward entrypoints that link to one full journey.

- [ ] **Step 1: Replace unsupported installation channels**

In `docs/getting-started/installation.md`:

- keep Cargo, the Unix install script, GitHub release assets, and PyPI `assay-it`;
- state that `assay-it` is the SDK/pytest plugin and does not install the CLI;
- remove Homebrew, Scoop, Docker, their uninstall/PATH notes, and GHCR examples;
- use `assay-v5.1.0-x86_64-pc-windows-msvc.zip` for Windows;
- change permission troubleshooting to `python -m pip install --user assay-it` and remove `pipx install assay`;
- keep `assay 5.1.0` as the exact expected output.

Also correct active MkDocs pages outside the getting-started section: `docs/python-sdk/index.md` may use `pip install assay-it` only for the SDK/plugin, and `docs/use-cases/air-gapped.md` must use a verified release asset or locally built image rather than claim a published GHCR image.

Correct outward agent context in `docs/AIcontext/user-flows.md` so CLI installation and Python SDK installation are separate. Add a dated correction to the historical `docs/migration-v1.2.md` Python package paragraph: the shipped SDK package is `assay-it`; do not silently rewrite the original release context.

- [ ] **Step 2: Collapse copied quickstarts**

Make `docs/getting-started/index.md` and `docs/getting-started/quickstart.md` link prominently to `../guides/agent-golden-path.md`. Their local examples may select one supported entry path, but must not copy all nine steps.

Use Cargo or the verified install script for CLI installation. Use `pip install assay-it` only in a separately labelled Python SDK paragraph. Replace “zero-flake” and similar absolute claims with observable language such as “replay recorded behavior deterministically when the required trace inputs are present.”

- [ ] **Step 3: Fix command and action examples**

In `docs/reference/cli/index.md`, replace `# assay 0.9.0` with `# assay 5.1.0` and verify command names against `assay --help` from the release binary.

In CI examples, pin third-party actions to the repository's current documented policy rather than `actions/checkout@v4`. Do not claim a GHCR image exists.

Keep root `README.md` concise: current release/install links, one canonical journey link, and no “New in 3.30.0” as the primary current-feature banner.

- [ ] **Step 4: Run the active-doc contract**

Run:

```bash
bash scripts/ci/test-check-release-surface.sh
ASSAY_BIN=target/debug/assay bash scripts/ci/check-release-surface.sh
python3 scripts/docs/generate-agent-golden-path.py --check
```

Expected: PASS.

- [ ] **Step 5: Commit the green guard and public entrypoints together**

```bash
git add -- scripts/ci/test-check-release-surface.sh scripts/ci/check-release-surface.sh .pre-commit-config.yaml README.md docs/index.md docs/getting-started/index.md docs/getting-started/installation.md docs/getting-started/quickstart.md docs/getting-started/ci-integration.md docs/reference/cli/index.md docs/python-sdk/index.md docs/use-cases/air-gapped.md docs/AIcontext/user-flows.md docs/migration-v1.2.md
git commit -m "docs: align install and quickstart with v5.1.0"
```

### Task 4: Reconcile plugin, skill, and MCP guidance

**Files:**
- Modify: `docs/guides/editor-mcp-recipe.md`
- Verify unchanged: `.mcp.json`
- Verify unchanged: `.cursor/mcp.json`
- Verify unchanged: `.claude-plugin/marketplace.json`
- Verify unchanged: `packaging/claude-plugin/.mcp.json`

**Interfaces:**
- Consumes: shipped manifests and the five-tool release contract.
- Produces: host-specific instructions with no unverified marketplace or cwd claim.

- [ ] **Step 1: Correct editor guidance**

Document separately:

```text
CLI              assay
MCP server       assay-mcp-server --policy-root .
Project skill    .agents/skills/assay-golden-path/SKILL.md
Claude plugin    packaging/claude-plugin via .claude-plugin/marketplace.json
```

State that `.` is resolved from the working directory supplied by the host. Remove the hard-coded Claude Code `2.1.32` statement and use vendor docs without pinning a client version unless the behavior was actually measured on that version.

State that plain stdio mode exposes tools but does not imply `proxy-enforce` policy enforcement. Name the five release tools; exclude `assay_test_outbound`.

- [ ] **Step 2: Verify manifest and packaged parity**

Run:

```bash
cmp .mcp.json .cursor/mcp.json
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-claude-plugin-install.sh --self-test
```

Expected: PASS. Do not force the plugin-local `.mcp.json` to be byte-identical when its packaged path semantics differ.

- [ ] **Step 3: Build active documentation strictly**

Run:

```bash
python3 -m venv /tmp/assay-docs-v51
/tmp/assay-docs-v51/bin/pip install -r docs/requirements-ci.txt
/tmp/assay-docs-v51/bin/mkdocs build --strict
git diff --check
```

Expected: PASS.

- [ ] **Step 4: Commit the host guidance**

```bash
git add -- docs/guides/editor-mcp-recipe.md
git commit -m "docs(integrations): align plugin skill and MCP guidance"
```

### Task 5: Final verification and review packet

**Files:**
- No new production files.

**Interfaces:**
- Consumes: all Slice 1 commits.
- Produces: exact-head verification evidence for the PR.

- [ ] **Step 1: Run focused and integration checks**

```bash
bash -c 'cargo build -p assay-cli && ASSAY_BIN="$(pwd)/target/debug/assay" bash scripts/ci/check-release-surface.sh'
bash scripts/ci/test-check-release-surface.sh
python3 scripts/docs/generate-agent-golden-path.py --check
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-claude-plugin-install.sh --self-test
cargo test -p assay-cli --test agent_golden_path_contract
cargo test -p assay-mcp-server --test agent_golden_path_contract
cargo fmt --all -- --check
cargo clippy -p assay-cli -p assay-mcp-server -- -D warnings
pre-commit run --all-files
git diff --check origin/main...HEAD
```

Expected: all pass.

- [ ] **Step 2: Re-run representative mutations**

Require the release-surface self-test to report all six mutations and the existing golden-path hardening suite to report its expected case count:

```bash
bash scripts/ci/test-check-release-surface.sh
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

- [ ] **Step 3: Open a draft PR and request one non-building exact-head review**

Record the exact SHA, commands, released-vs-unreleased boundary, verified channels, and explicit non-claims. Do not count the builder's review or an automated reviewer toward quorum.
