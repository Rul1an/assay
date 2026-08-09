# Golden-Path Durability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the generated golden-path skill gates CI-scheduled, structurally pinned, checkout-stable, non-destructive, and mutation-proven before plugin packaging begins.

**Architecture:** Keep `scripts/ci/test-agent-golden-path-skill.py` as the single validator and grow one bounded workflow model that serves path scheduling and executor checks. Keep hostile mutations in disposable repositories, make Git probes independent of host configuration, and make the drift self-test operate only on scratch copies. PR A must not change any generated artifact bytes.

**Tech Stack:** Python 3.12-compatible stdlib, Bash with `set -euo pipefail`, Git CLI, pre-commit 4.4.0, GitHub Actions YAML, Cargo-driven golden-path integration tests.

## Global Constraints

- Work only in `/Users/roelschuurkes/.config/superpowers/worktrees/assay/2176-durability` on `codex/2176-golden-path-durability`.
- Use `CARGO_TARGET_DIR=/tmp/assay-2176-durability-target`; do not share another worktree's target directory.
- Before Task 1, record the Claude-approved plan head as `PLAN_HEAD`; base all
  implementation behavior on the design file at that exact head and issue
  #2176's latest comments.
- Before every production change, add a named mutation, run it, and confirm the expected RED.
- Every mutation writes only inside `mktemp -d`; never rewrite a tracked repository file for a test.
- Read bounded evidence before decoding or parsing it.
- Use `parse_kernel_matrix_workflow(text: str) -> WorkflowContract` as the one workflow rule source.
- Keep both shipped skills regular, tracked, ASCII, byte-identical files with `eol=lf`.
- PR A must leave all seven generated artifacts byte-identical to base `55e8103029583a16af96e1eafe73025192d69cd2`.
- Stage only the exact paths listed in each task's commit command.
- Bind every reported pass/count/digest and every reviewer verdict to the exact committed head SHA.
- Final-head review requires the existing context-bearing Claude Code Desktop chat plus the repository's second reviewer/bot route under `AGENTS.md`.

## File Map

- Modify `.pre-commit-config.yaml`: schedule and trigger the drift self-test and both golden-path gates.
- Modify `.github/workflows/kernel-matrix.yml`: schedule `.gitattributes` changes and remain the structurally validated executor.
- Modify `.gitignore`: ignore arbitrary siblings under the shipped Claude skill directory.
- Modify `.gitattributes`: force LF for both exact-byte skill payloads.
- Modify `scripts/ci/test-agent-golden-path-skill.py`: bounded pre-commit parsing, workflow model, executor checks, Git probes, and symlink ordering.
- Modify `scripts/ci/test-agent-golden-path-skill-hardening.sh`: direct parser and mutation battery in isolated Git repositories.
- Modify `scripts/ci/test-agent-golden-path-skill-optimization.sh`: AST-wide no-assert proof and optimized-Python mutation.
- Create `scripts/ci/test-check-docs-generated-drift-safety.sh`: prove the drift self-test does not rely on cleanup to restore its repository.
- Rewrite `scripts/ci/test-check-docs-generated-drift.sh`: run every mutation in scratch and snapshot the reviewable tree.
- Modify `scripts/docs/generate-agent-golden-path.py`: replace optimizer-erased assertions with explicit typed checks without changing output bytes.
- Verify `docs/superpowers/specs/2026-08-09-golden-path-hardening-design.md`:
  implementation must preserve its reviewed executor predicate and Non-Goals.
- Create no generated artifact and modify none of `.agents/skills/assay-golden-path/SKILL.md`, `.claude/skills/assay-golden-path/SKILL.md`, `docs/generated/agent-golden-path.json`, `docs/generated/*.mermaid`, `docs/AIcontext/architecture-diagrams.md`, or `docs/guides/agent-golden-path.md`.

---

### Task 1: Schedule And Pin The Drift Self-Test Hook

**Files:**
- Modify: `.pre-commit-config.yaml:103-135`
- Modify: `scripts/ci/test-agent-golden-path-skill.py:11-18,124-242`
- Modify: `scripts/ci/test-agent-golden-path-skill-hardening.sh:8-97`

**Interfaces:**
- Produces: `parse_precommit_self_test(text: str) -> PrecommitHookContract`.
- Produces: `PrecommitHookContract(stages: tuple[str, ...] | None, files_pattern: str)`.
- Produces one hardening-battery counter shared by Tasks 1-5. Every named
  failure, allow probe, or explicit platform skip increments it exactly once;
  the script fails unless the final count equals the task's pinned expectation.
- Consumes later: Task 4 extends the same trigger assertions with `.gitattributes`.

- [ ] **Step 1: Seed pre-commit evidence in hardening cases**

Extend `seed_case()` with `.pre-commit-config.yaml` and add the bounded path in the validator:

```python
PRECOMMIT_PATH = ROOT / ".pre-commit-config.yaml"


@dataclass(frozen=True)
class PrecommitHookContract:
    stages: tuple[str, ...] | None
    files_pattern: str
```

The shell seed must create the destination root and copy the file:

```bash
cp "$ROOT/.pre-commit-config.yaml" "$case_root/"
```

Refactor the existing nine cases through the same accounting path before adding
new ones. Task 1 pins `EXPECTED_CASES=11`: nine existing cases plus the two new
scheduling mutations. `expect_named_failure()` increments only after it observes
the named diagnostic; allow probes use `record_case_pass()`. The final success
line prints `agent golden-path hardening: 11 case(s) executed` and refuses zero,
missing, or extra cases.

- [ ] **Step 2: Add two failing scheduling mutations**

For separate seeded cases:

1. force `stages: [pre-push]` on `docs-generated-drift-self-test` and require
   `generated-docs drift self-test must run at the default pre-commit stage`;
2. remove `scripts/docs/generate-agent-golden-path\.py` from that hook's `files`
   regex and require
   `generated-docs drift self-test does not cover its golden-path generator`.

Use exact, unique text replacement and abort the mutation setup if the source
fragment count is not one.

- [ ] **Step 3: Run the hardening script and record RED**

Run:

```bash
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: non-zero because the current validator accepts at least the
pre-push-only mutation. Record the named `was accepted` case in the task log.

- [ ] **Step 4: Implement bounded hook parsing**

Implement a line/indentation parser that selects exactly one active
`- id: docs-generated-drift-self-test` block under local hooks. Comments do not
count. Parse only the active `stages` inline list and `files` scalar; reject
missing or duplicate keys.

Validate with these exact predicates:

```python
hook = parse_precommit_self_test(precommit_text)
if hook.stages is not None and "pre-commit" not in hook.stages:
    fail("generated-docs drift self-test must run at the default pre-commit stage")
if re.search(hook.files_pattern, "scripts/docs/generate-agent-golden-path.py") is None:
    fail("generated-docs drift self-test does not cover its golden-path generator")
```

- [ ] **Step 5: Change the hook to default-stage and cover the generator**

Remove `stages: [pre-push]`. Extend `files:` to include
`scripts/docs/generate-agent-golden-path\.py` while retaining the gate script,
self-test script, and both skill destinations.

- [ ] **Step 6: Run focused GREEN verification**

Run:

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: both pass, the two scheduling mutation names print `ok`, and the
hardening script reports exactly 11 executed cases. Do not
execute `docs-generated-drift-self-test` against this worktree yet: until Task 6
lands, the base self-test still mutates tracked files and relies on an EXIT trap
to restore them. The actual default-stage invocation is exercised in Task 8,
after the self-test is scratch-only.

- [ ] **Step 7: Commit Task 1**

```bash
git add -A -- .pre-commit-config.yaml \
  scripts/ci/test-agent-golden-path-skill.py \
  scripts/ci/test-agent-golden-path-skill-hardening.sh
git commit -m "test(agent): schedule golden-path drift self-test in CI"
```

---

### Task 2: Parse Pull-Request Activation Once

**Files:**
- Modify: `scripts/ci/test-agent-golden-path-skill.py:54-92,226-240`
- Modify: `scripts/ci/test-agent-golden-path-skill-hardening.sh`

**Interfaces:**
- Replaces: `workflow_pull_request_paths(text: str) -> set[str]`.
- Produces: `parse_kernel_matrix_workflow(text: str) -> WorkflowContract`.
- Produces immutable fields:

```python
@dataclass(frozen=True)
class WorkflowStepContract:
    condition: str | None
    continue_on_error: bool | None
    shell_lines: tuple[str, ...]


@dataclass(frozen=True)
class WorkflowContract:
    pull_request_branches: tuple[str, ...]
    pull_request_types: tuple[str, ...] | None
    pull_request_paths: tuple[str, ...]
    lint_runner: str
    lint_needs: tuple[str, ...] | None
    lint_condition: str | None
    lint_continue_on_error: bool | None
    lint_steps: tuple[WorkflowStepContract, ...]
```

- Consumes later: Task 3 validates `lint_*` and `lint_steps` without reparsing text.

- [ ] **Step 1: Add direct parser mutation cases**

Add scratch cases that require these diagnostics:

| Mutation | Exact diagnostic fragment |
|---|---|
| Tab before a `paths` item | `kernel-matrix workflow uses tab indentation` |
| Duplicate `paths:` key | `kernel-matrix pull_request duplicates key: paths` |
| Delete `paths:` | `kernel-matrix pull_request is missing required key: paths` |
| Unsupported unquoted path item | `kernel-matrix pull_request.paths contains an unsupported entry` |
| Add `paths-ignore:` | `kernel-matrix pull_request cannot combine paths and paths-ignore` |
| Add `branches-ignore:` | `kernel-matrix pull_request cannot combine branches and branches-ignore` |
| Change branches to `release/*` | `kernel-matrix pull_request does not cover main` |
| Change branches to unquoted `[main]` | `kernel-matrix pull_request.branches must be a bracketed list of quoted strings` |
| Change branches to a block sequence | `kernel-matrix pull_request.branches must be a bracketed list of quoted strings` |
| Add `types: [labeled]` | `kernel-matrix pull_request must not declare types` |
| Comment out `pull_request:` | `kernel-matrix workflow must declare exactly one pull_request section` |
| Delete lint `runs-on:` | `kernel-matrix lint job is missing required key: runs-on` |

The `types` diagnostic says the key is forbidden, not that it is narrowed.

Add one direct parser probe for the existing inline `run:` form: replace
`run: cargo install --locked cargo-deny` in a scratch workflow with
`run: echo inline-parser-sentinel`, import the copied validator without invoking
`main()`, call `parse_kernel_matrix_workflow()`, and require exactly one step
whose `shell_lines == ("echo inline-parser-sentinel",)`. Also require every
`uses:`-only step to have empty `shell_lines`. This probe fails if inline runs
are silently discarded while the main workflow still happens to validate.
Update `EXPECTED_CASES` from 11 to 24: twelve named parser failures plus this
one allow probe are added to the eleven Task 1 cases.

- [ ] **Step 2: Run parser mutations and confirm RED**

Run:

```bash
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: non-zero; at minimum `paths-ignore`, `branches-ignore`, branch, and
types mutations are accepted or reach the old generic coverage diagnostic.

- [ ] **Step 3: Implement the bounded workflow parser**

Use these helpers in the validator:

```python
def indentation(line: str) -> int:
    return len(line) - len(line.lstrip(" "))


def active_lines(text: str) -> list[tuple[int, str]]:
    result = []
    for line_number, line in enumerate(text.splitlines(), start=1):
        prefix = line[: len(line) - len(line.lstrip())]
        if "\t" in prefix:
            fail(f"kernel-matrix workflow uses tab indentation at line {line_number}")
        stripped = line.strip()
        if stripped and not stripped.startswith("#"):
            result.append((line_number, line))
    return result
```

Implement `parse_kernel_matrix_workflow()` with this exact sequence:

1. call `active_lines()`;
2. locate one `on:` mapping at indentation 0 and one nested `pull_request:` at
   indentation 2;
3. collect its direct indentation-4 keys, rejecting duplicates before parsing
   values;
4. reject `paths-ignore` with `paths` and `branches-ignore` with `branches`;
5. parse `branches` and optional `types` through one
   `parse_inline_string_list(raw, label)` helper; require `[`/`]`, wrap
   `ast.literal_eval` in `try/except (SyntaxError, ValueError)`, and fail with
   `<label> must be a bracketed list of quoted strings` before rejecting any
   non-string element with the same named diagnostic;
6. parse the indentation-6 quoted `paths` sequence until the next active line
   at indentation 4 or less;
7. locate one `jobs:` mapping, one nested `lint:` mapping, and collect its
   direct scalar keys plus its indentation-6 step sequence;
8. for each step, collect direct `if` and `continue-on-error`; `run` is optional,
   a `run: <inline scalar>` contributes exactly that one shell line, a `run: |`
   block contributes its expanded body, and a `uses:`-only step contributes an
   empty `shell_lines`; reject duplicate `run` keys and do not expand other
   block-scalar styles;
9. construct `WorkflowContract` only after every required section and value has
   passed validation.

Reject every non-string list element. A helper that locates a mapping receives
`(lines, start_index, parent_indent, key)` and returns the unique key line plus
the first following line at indentation less than or equal to the key; it fails
if zero or multiple active keys occur in that bounded section.

Shell block lines are retained separately. Blank lines and lines whose first
non-whitespace character is `#` are not active shell lines.

- [ ] **Step 4: Replace path coverage consumption**

In `main()`:

```python
workflow_contract = parse_kernel_matrix_workflow(workflow)
workflow_paths = set(workflow_contract.pull_request_paths)
if "main" not in workflow_contract.pull_request_branches:
    fail("kernel-matrix pull_request does not cover main")
if workflow_contract.pull_request_types is not None:
    fail("kernel-matrix pull_request must not declare types")
```

The parser itself rejects active `paths-ignore` and `branches-ignore` siblings.

- [ ] **Step 5: Run focused GREEN verification**

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
python3 -OO scripts/ci/test-agent-golden-path-skill.py
```

Expected: pass under normal and optimized Python; all direct parser mutations
reach their named diagnostic and the hardening script reports exactly 24 cases.

- [ ] **Step 6: Commit Task 2**

```bash
git add -A -- scripts/ci/test-agent-golden-path-skill.py \
  scripts/ci/test-agent-golden-path-skill-hardening.sh
git commit -m "test(agent): parse golden-path workflow activation structurally"
```

---

### Task 3: Pin The Canonical Lint Executor

**Files:**
- Modify: `scripts/ci/test-agent-golden-path-skill.py`
- Modify: `scripts/ci/test-agent-golden-path-skill-hardening.sh`

**Interfaces:**
- Consumes: `WorkflowContract` from Task 2.
- Produces: `validate_lint_executor(contract: WorkflowContract) -> None`.

- [ ] **Step 1: Add the executor mutation matrix**

Create one scratch case per mutation and require the diagnostic shown:

| Mutation | Exact diagnostic fragment |
|---|---|
| Delete or comment `lint:` | `kernel-matrix workflow must declare exactly one active lint job` |
| Delete or comment canonical command | `kernel-matrix lint job has no canonical pre-commit executor` |
| Replace `--all-files` with `--files` | `kernel-matrix lint pre-commit command is noncanonical` |
| Append `--hook-stage pre-push` to the canonical command | `kernel-matrix lint pre-commit command is noncanonical` |
| Add job or executor-step `if: false` | `kernel-matrix lint executor must not be conditional` |
| Add `continue-on-error: true` | `kernel-matrix lint executor must fail closed` |
| Add `needs: optional-job` | `kernel-matrix lint job must not depend on another job` |
| Change `runs-on` | `kernel-matrix lint job must run on ubuntu-latest` |

Also add one allow case with a separate step that runs
`pre-commit run --hook-stage pre-push --all-files`; it must not replace or
invalidate the canonical default-stage step.
Update `EXPECTED_CASES` from 24 to 33 for eight named failures plus this allow
case.

- [ ] **Step 2: Run mutations and confirm RED**

```bash
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: non-zero because the Task 2 parser returns executor data but no
consumer yet rejects the mutations.

- [ ] **Step 3: Implement exact active-line validation**

```python
CANONICAL_PRECOMMIT = "pre-commit run --all-files --show-diff-on-failure"


def validate_lint_executor(contract: WorkflowContract) -> None:
    if contract.lint_runner != "ubuntu-latest":
        fail("kernel-matrix lint job must run on ubuntu-latest")
    if contract.lint_needs is not None:
        fail("kernel-matrix lint job must not depend on another job")
    if contract.lint_condition is not None:
        fail("kernel-matrix lint executor must not be conditional")
    if contract.lint_continue_on_error is True:
        fail("kernel-matrix lint executor must fail closed")

    invocation_steps = []
    canonical_steps = []
    for step in contract.lint_steps:
        active = tuple(line.strip() for line in step.shell_lines if line.strip())
        invocations = tuple(line for line in active if line.startswith("pre-commit "))
        if invocations:
            invocation_steps.append((step, invocations))
        if CANONICAL_PRECOMMIT in invocations:
            canonical_steps.append((step, invocations))

    if not invocation_steps:
        fail("kernel-matrix lint job has no canonical pre-commit executor")
    if len(canonical_steps) != 1:
        fail("kernel-matrix lint pre-commit command is noncanonical")

    step, invocations = canonical_steps[0]
    if step.condition is not None:
        fail("kernel-matrix lint executor must not be conditional")
    if step.continue_on_error is True:
        fail("kernel-matrix lint executor must fail closed")
    if invocations != (CANONICAL_PRECOMMIT,):
        fail("kernel-matrix lint pre-commit command is noncanonical")
```

Do not claim to interpret heredocs or arbitrary shell reachability. Comment-only
lines are already absent from `shell_lines`; an inline suffix does not equal the
canonical command.

Add a validator comment that `ubuntu-latest` is an intentional maintenance pin:
a runner migration must update this contract and its mutation proof together.

- [ ] **Step 4: Run focused GREEN verification**

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
pre-commit run agent-golden-path-skill-contract --all-files
```

Expected: the hardening script reports exactly 33 cases.

- [ ] **Step 5: Commit Task 3**

```bash
git add -A -- scripts/ci/test-agent-golden-path-skill.py \
  scripts/ci/test-agent-golden-path-skill-hardening.sh
git commit -m "test(agent): pin golden-path CI executor"
```

---

### Task 4: Enforce Git Tracking, Ignore Scope, And LF Bytes

**Files:**
- Modify: `.gitignore:11-18`
- Modify: `.gitattributes:1-4`
- Modify: `.github/workflows/kernel-matrix.yml:11-32`
- Modify: `.pre-commit-config.yaml:103-135`
- Modify: `scripts/ci/test-agent-golden-path-skill.py`
- Modify: `scripts/ci/test-agent-golden-path-skill-hardening.sh`

**Interfaces:**
- Produces: `run_git(*args: str) -> subprocess.CompletedProcess[str]`.
- Produces: `validate_skill_repository_state() -> None`.

- [ ] **Step 1: Turn every hardening case into a hermetic Git repository**

After seeding fixtures, initialize and index with:

```bash
env GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
  git -C "$case_root" -c init.defaultBranch=main init -q
env GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null \
  git -C "$case_root" -c core.excludesFile= -c core.attributesFile= \
  add -f -- .
```

Copy `.gitignore` and `.gitattributes` in `seed_case()`.
Keep `-f`: ignored shipped paths must still be indexed so the independent
`check-ignore --no-index` mutation can prove ignore state without conflating it
with tracking state.

- [ ] **Step 2: Add independent Git-state mutations**

Require named failures for:

- `git rm --cached` on each skill independently;
- adding an ignore rule for each shipped skill and checking with `--no-index`;
- removing the inner wildcard so `OTHER.md` becomes visible;
- deleting each skill's `eol=lf` line independently;
- setting hostile temporary global `core.excludesFile` and
  `core.attributesFile` values in two separate control cases and proving neither
  can satisfy the case.

Together with the four trigger-chain mutations in Step 6, Task 4 adds thirteen
cases. Update `EXPECTED_CASES` from 33 to 46.

Named diagnostics:

```text
skill is not tracked: <path>
tracked skill is ignored: <path>
Claude skill sibling is not ignored: .claude/skills/assay-golden-path/OTHER.md
skill does not declare eol=lf: <path>
```

- [ ] **Step 3: Run Git-state mutations and confirm RED**

```bash
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: non-zero; current validator never asks Git and `.gitattributes` has
no skill entries.

- [ ] **Step 4: Implement hermetic Git probes**

```python
GIT_ENV = {
    **os.environ,
    "GIT_CONFIG_NOSYSTEM": "1",
    "GIT_CONFIG_GLOBAL": os.devnull,
    "GIT_CEILING_DIRECTORIES": str(ROOT.parent),
}


def run_git(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [
            "git",
            "-c", "core.excludesFile=",
            "-c", "core.attributesFile=",
            "-c", "init.defaultBranch=main",
            *args,
        ],
        cwd=ROOT,
        env=GIT_ENV,
        text=True,
        capture_output=True,
        check=False,
    )
```

Call only with constant paths from `SKILL_PATHS` relative to `ROOT`. Call
`validate_skill_repository_state()` in `main()` before the first
`read_bounded_evidence()` call for either skill, so an indexed symlink first
passes the tracking predicate and then reaches the file-type predicate. Require:

```python
git ls-files --error-unmatch -- <skill>
git check-ignore --no-index -- <skill>          # must be non-zero
git check-ignore --no-index -- <OTHER.md>        # must be zero
git check-attr eol -- <skill>                    # value must be lf
```

- [ ] **Step 5: Narrow ignore rules and add LF attributes**

Use this exact nested pattern:

```gitignore
!.claude/skills/assay-golden-path/
.claude/skills/assay-golden-path/*
!.claude/skills/assay-golden-path/SKILL.md
```

Append exact attributes:

```gitattributes
.agents/skills/assay-golden-path/SKILL.md text eol=lf
.claude/skills/assay-golden-path/SKILL.md text eol=lf
```

- [ ] **Step 6: Close the trigger chain**

Add `.gitattributes` to:

- `kernel-matrix.yml` `pull_request.paths`;
- `agent-golden-path-skill-contract` `files`;
- `docs-generated-drift` `files`.

Extend `required_workflow_paths` with `.gitattributes` and add mutations that
remove/comment the workflow path and remove it from both hook regexes.

- [ ] **Step 7: Run focused GREEN verification**

```bash
export GIT_CONFIG_NOSYSTEM=1
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CEILING_DIRECTORIES="$(dirname "$PWD")"
git -c core.excludesFile= -c core.attributesFile= \
  check-ignore --no-index .claude/skills/assay-golden-path/OTHER.md
! git -c core.excludesFile= -c core.attributesFile= \
  check-ignore --no-index .claude/skills/assay-golden-path/SKILL.md
git -c core.excludesFile= -c core.attributesFile= \
  ls-files --error-unmatch -- \
  .agents/skills/assay-golden-path/SKILL.md \
  .claude/skills/assay-golden-path/SKILL.md
git -c core.excludesFile= -c core.attributesFile= check-attr eol -- \
  .agents/skills/assay-golden-path/SKILL.md \
  .claude/skills/assay-golden-path/SKILL.md
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: the hardening script reports exactly 46 cases.

- [ ] **Step 8: Commit Task 4**

```bash
git add -A -- .gitignore .gitattributes .github/workflows/kernel-matrix.yml \
  .pre-commit-config.yaml scripts/ci/test-agent-golden-path-skill.py \
  scripts/ci/test-agent-golden-path-skill-hardening.sh
git commit -m "test(agent): pin golden-path repository state"
```

---

### Task 5: Reject Both Skill Symlink Classes

**Files:**
- Modify: `scripts/ci/test-agent-golden-path-skill.py:40-51`
- Modify: `scripts/ci/test-agent-golden-path-skill-hardening.sh`

**Interfaces:**
- Preserves: `read_bounded_evidence(path: Path, label: str) -> bytes`.
- Changes: file-type check order only.

- [ ] **Step 1: Add per-destination symlink mutations**

Create independent scratch cases:

1. replace the Codex skill with a symlink to a regular copied file;
2. replace the Claude skill with a dangling symlink;
3. replace one destination with a symlink to a directory.

After creating each symlink, force-add it to the scratch index so the tracking
guard passes and the file-type guard is the named failure. Require:

```text
skill evidence must be a regular tracked file, not a symlink
```

Skip with an explicit platform diagnostic only if the operating system refuses
symlink creation; CI on Ubuntu must execute all cases.
Update `EXPECTED_CASES` from 46 to 49. A platform skip counts as an observed
case and is printed as `skip`, never as `ok`; Ubuntu must report all three as
executed rather than skipped.

- [ ] **Step 2: Run mutations and confirm the ordering RED**

```bash
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: dangling/directory cases report `is missing` rather than the required
symlink diagnostic.

- [ ] **Step 3: Move the symlink check before `is_file()`**

```python
def read_bounded_evidence(path: Path, label: str) -> bytes:
    if path.is_symlink():
        fail(f"{label} must be a regular tracked file, not a symlink: {path}")
    if not path.is_file():
        fail(f"{label} is missing: {path.relative_to(ROOT)}")
    if path.stat().st_size > MAX_EVIDENCE_BYTES:
        fail(f"{label} exceeds {MAX_EVIDENCE_BYTES}-byte limit")
    with path.open("rb") as handle:
        payload = handle.read(MAX_EVIDENCE_BYTES + 1)
    if len(payload) > MAX_EVIDENCE_BYTES:
        fail(f"{label} exceeds {MAX_EVIDENCE_BYTES}-byte limit")
    return payload
```

- [ ] **Step 4: Run focused GREEN verification**

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
```

Expected: the hardening script reports exactly 49 observed cases, with no
symlink skip on Ubuntu.

- [ ] **Step 5: Commit Task 5**

```bash
git add -A -- scripts/ci/test-agent-golden-path-skill.py \
  scripts/ci/test-agent-golden-path-skill-hardening.sh
git commit -m "test(agent): reject symlinked golden-path skills"
```

---

### Task 6: Make The Drift Mutation Battery Scratch-Only

**Files:**
- Create: `scripts/ci/test-check-docs-generated-drift-safety.sh`
- Rewrite: `scripts/ci/test-check-docs-generated-drift.sh`
- Modify: `.pre-commit-config.yaml:103-112`

**Interfaces:**
- Produces shell functions:
  - `seed_repo(destination: str)`
  - `snapshot_tree(root: str) -> manifest on stdout`
  - `run_gate(case_root: str) -> exit status`
  - `expect_gate_status(name: str, case_root: str, expected: int)`
- The safety wrapper consumes the self-test as a subprocess and proves it does
  not rely on cleanup after an interrupted mutation to restore the repository it
  runs from.
- The self-test accepts the test-only selector
  `ASSAY_DOCS_DRIFT_SELF_TEST_CASE`; unset runs the full battery and
  `hand-edited-diagram` runs exactly that one named case. Unknown values fail.
- The self-test accepts `ASSAY_DOCS_DRIFT_INTERRUPT_AFTER_MUTATION` only when it
  equals the selected case. At the named point after planting the defect but
  before running the gate, it prints an interruption marker and exits 97.

- [ ] **Step 1: Write the interrupted-run safety test**

The new script must:

1. copy the tracked working tree to `mktemp -d/case/repo` using the same
   `git ls-files -z | tar --null -T -` pattern as the drift gate;
2. detect exactly one mode: historical `trap cleanup EXIT` or target
   `trap 'rm -rf "$SCRATCH"' EXIT`; absence, duplication, or both forms is a named
   failure, so the probe cannot silently disarm itself;
3. in historical mode, replace the trap with `trap : EXIT` and inject an exact,
   unique `echo "test interruption: hand-edited-diagram"; exit 97` immediately
   after the first diagram mutation and before its `check` call and inline
   restore. In target mode, replace the scratch trap with `trap : EXIT` and use
   the built-in interruption environment variable instead of source injection;
4. in target mode, structurally reject backup-and-restore code in the copied
   self-test, including a `cleanup()` function, `*_BACKUP` variables,
   `trap cleanup`, or a `cp`/`mv` destination rooted under `$ROOT` outside
   `seed_repo()`; historical mode remains accepted only to drive the initial RED;
5. snapshot the copied repository's tracked and non-ignored untracked entries;
6. run with `TMPDIR` set to a second disposable directory outside the copied
   repository. In target mode also set both test-only variables to
   `hand-edited-diagram`. Require exit 97 and exactly one
   `test interruption: hand-edited-diagram` marker, then remove the external temp
   directory;
7. snapshot again and fail with
   `drift self-test writes its repository before cleanup` if the manifests
   differ.

The snapshot manifest is generated by embedded Python from the output of a
hermetic `git ls-files -z --cached --others --exclude-standard` invocation with
`GIT_CONFIG_NOSYSTEM=1`, `GIT_CONFIG_GLOBAL=/dev/null`, empty
`core.excludesFile`/`core.attributesFile`, and a ceiling at the copied repo's
parent. For every path, hash a record containing path bytes, `lstat` file type,
regular-file content, or symlink target. Sort by raw path bytes before hashing.

- [ ] **Step 2: Run the safety test and confirm RED**

```bash
bash scripts/ci/test-check-docs-generated-drift-safety.sh
```

Expected: non-zero with
`drift self-test writes its repository before cleanup`, because the historical
probe interrupts after the first mutation and before both its inline restore and
its now-disabled EXIT restore. A normal uninterrupted historical run is expected
to leave the tree clean and is not the RED claim.

- [ ] **Step 3: Rewrite the self-test around a scratch seed**

Start with:

```bash
set -euo pipefail
ROOT="$(git rev-parse --show-toplevel)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
SEED="$SCRATCH/seed"

seed_repo "$SEED"
ROOT_BEFORE="$(snapshot_tree "$ROOT")"
```

For every case, copy `SEED/.` to a fresh case root, mutate only that copy, and
execute `case_root/scripts/ci/check-docs-generated-drift.sh` with cwd at the
case root. Capture expected non-zero statuses explicitly inside `if`; do not
disable `set -e` globally.

Route named cases through one case table. Increment `executed_cases` after each
selected case, print `generated-docs drift self-test: <n> case(s) executed`, and
require `n == 1` when the test-only selector is set. This makes the safety
wrapper distinguish a passing case from a selector that ran nothing.

Before each selected mutation, print `running drift case: <name>`. Immediately
after planting the named defect and before invoking its gate, call one
`maybe_interrupt_after_mutation(name)` helper. The helper validates the
test-only environment value and emits the exact interruption marker before exit
97. This is the persistent GREEN path exercised by the safety wrapper; because
the case root is under `SCRATCH`, interruption cannot dirty the copied repo even
with its scratch cleanup trap disabled.

- [ ] **Step 4: Port all existing mutations and add both destinations**

Keep separate cases for:

- hand-edited crate dependency diagram;
- hand-edited machine contract;
- hand-edited rendered guide table;
- hand-edited Codex skill;
- generator unable to run;
- gate reads working tree rather than `HEAD`.

Add two generator-source cases. Remove exactly one tuple line at a time:

```python
'    ROOT / ".agents/skills/assay-golden-path/SKILL.md",\n'
'    ROOT / ".claude/skills/assay-golden-path/SKILL.md",\n'
```

Require the gate's named missing-output error for the corresponding path.

- [ ] **Step 5: Add tree snapshot proof and its meta-mutation**

After the full battery:

```bash
ROOT_AFTER="$(snapshot_tree "$ROOT")"
if [[ "$ROOT_BEFORE" != "$ROOT_AFTER" ]]; then
  echo "FAIL: generated-docs self-test changed the reviewable repository tree" >&2
  diff -u <(printf '%s\n' "$ROOT_BEFORE") <(printf '%s\n' "$ROOT_AFTER") >&2 || true
  exit 1
fi
```

In a separate scratch case, snapshot, append to the copied
`docs/generated/crate-deps.mermaid`, snapshot again, and require the manifest
diff to name that exact path. This proves the snapshot is neither constant nor
empty.

The real-root snapshot and the meta-mutation's snapshot use the same hermetic
Git environment and config overrides as Task 4; no host-global ignore file may
remove a path from the proof.

- [ ] **Step 6: Add the safety wrapper to the hook**

Keep the hook at the default stage and set its entry to:

```yaml
entry: bash -c 'bash scripts/ci/test-check-docs-generated-drift-safety.sh && bash scripts/ci/test-check-docs-generated-drift.sh'
```

Extend its `files` regex with the new safety script. The wrapper executes only
the one selector-gated mutation; the full battery therefore runs once, not once
inside the wrapper and once directly.

- [ ] **Step 7: Run focused GREEN verification**

```bash
bash scripts/ci/test-check-docs-generated-drift-safety.sh
bash scripts/ci/test-check-docs-generated-drift.sh
bash scripts/ci/check-docs-generated-drift.sh
pre-commit run docs-generated-drift-self-test --all-files
```

Expected: all pass and `git status --short` is unchanged before/after.

- [ ] **Step 8: Commit Task 6**

```bash
git add -A -- .pre-commit-config.yaml \
  scripts/ci/test-check-docs-generated-drift-safety.sh \
  scripts/ci/test-check-docs-generated-drift.sh
git commit -m "test(docs): isolate generated-drift mutations"
```

---

### Task 7: Make Optimizer Safety Property-Wide

**Files:**
- Modify: `scripts/ci/test-agent-golden-path-skill-optimization.sh`
- Modify: `scripts/docs/generate-agent-golden-path.py:367-372,390-395,473-476`

**Interfaces:**
- Produces generator helper:

```python
def require_type(value: object, expected: type, label: str):
    if not isinstance(value, expected):
        raise SystemExit(f"{label} must be {expected.__name__}")
    return value
```

- Preserves generated bytes exactly.

- [ ] **Step 1: Add AST no-assert checks for both Python files**

In the optimization script, copy both sources and run embedded Python:

```python
import ast
from pathlib import Path

if len(sys.argv) != 3:
    raise SystemExit("optimizer gate must scan exactly two Python files")
for raw in sys.argv[1:]:
    path = Path(raw)
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    asserts = [node for node in ast.walk(tree) if isinstance(node, ast.Assert)]
    if asserts:
        lines = ", ".join(str(node.lineno) for node in asserts)
        raise SystemExit(f"optimizer-erased assert in {path}: lines {lines}")
```

Scan `test-agent-golden-path-skill.py` and
`generate-agent-golden-path.py`. Embedded Python in Bash gates remains outside
this AST scan; that bounded optimizer-safety residual is explicit and is not a
claim that every Python fragment in the repository is assert-free.

- [ ] **Step 2: Run AST check and confirm RED**

```bash
bash scripts/ci/test-agent-golden-path-skill-optimization.sh
```

Expected: non-zero naming generator lines 369, 371, 392, and 475.

- [ ] **Step 3: Replace all four generator assertions**

Use explicit checks, for example:

```python
outcomes = require_type(step["outcomes"], list, "golden-path outcomes")
primary_outcome = require_type(
    outcomes[0], dict, "primary golden-path outcome"
)
primary_argv = require_type(
    primary_outcome["argv"], list, "primary golden-path argv"
)
```

Use `require_type()` for all four replacements, including `outcomes` in
`exit_summary()` and `non_claims` in `render_skill()`. Do not duplicate the
`isinstance` rule inline.

- [ ] **Step 4: Add the insert-assert mutation**

In a copied validator, replace one explicit schema guard with an `assert` and
require the AST check to fail with `optimizer-erased assert`. Retain the existing
`python3 -OO` invalid-schema mutation and its named
`unexpected golden-path contract schema` diagnostic.

- [ ] **Step 5: Prove generated bytes did not change**

Record before digests from `HEAD`, run the generator, and compare all generated
outputs written by this generator:

```bash
git diff --exit-code -- \
  .agents/skills/assay-golden-path/SKILL.md \
  .claude/skills/assay-golden-path/SKILL.md \
  docs/generated/agent-golden-path.json \
  docs/guides/agent-golden-path.md
```

Then run:

```bash
bash scripts/ci/test-agent-golden-path-skill-optimization.sh
bash scripts/ci/check-docs-generated-drift.sh
```

This focused four-path proof is intentionally narrower than the seven-path
PR-A byte-identity assertion; Task 8 separately compares all seven paths to the
base commit.

- [ ] **Step 6: Commit Task 7**

```bash
git add -A -- scripts/ci/test-agent-golden-path-skill-optimization.sh \
  scripts/docs/generate-agent-golden-path.py
git commit -m "test(agent): preserve golden-path guards under optimization"
```

---

### Task 8: Run The PR-A Gate

**Files:**
- Verify only: the reviewed design, all Task 1-7 paths, and existing golden-path
  integration tests.

**Interfaces:**
- Produces no runtime or generated interface.
- Carries forward PR B notes without implementing PR B.

- [ ] **Step 1: Verify the reviewed design boundaries remain intact**

Require `git diff --exit-code "$PLAN_HEAD" --` for the design file. Verify these
three adjacent Non-Goals remain separate bullets:

```markdown
- No Cursor runtime-discovery claim or Cursor-specific skill copy.
- No new Python version source, launcher, or duplicated toolchain pin.
- No runtime behavior, command output schema, exit code, policy, evidence, or
  security semantics change.
```

Also verify the executor rule says the shell body has one active `pre-commit`
invocation equal to the canonical command while allowing other setup lines. Do
not edit the design during implementation, and do not add future-plugin or
packaging language to either generated skill.

- [ ] **Step 2: Run focused script gates**

```bash
python3 scripts/ci/test-agent-golden-path-skill.py
bash scripts/ci/test-agent-golden-path-skill-hardening.sh
bash scripts/ci/test-agent-golden-path-skill-optimization.sh
bash scripts/ci/test-check-docs-generated-drift-safety.sh
bash scripts/ci/test-check-docs-generated-drift.sh
bash scripts/ci/check-docs-generated-drift.sh
```

- [ ] **Step 3: Run user/workflow simulation**

Create a clean tracked-files scratch checkout and execute the exact workflow
command:

```bash
pre-commit run --all-files --show-diff-on-failure
```

Then, in a second scratch checkout, restore
`stages: [pre-push]` on the self-test and run the contract hook. Require the
named default-stage diagnostic. Record both commands, statuses, exact branch
head SHA, Python version, and pre-commit version.

- [ ] **Step 4: Run existing binary-level and AI/workflow contract tests**

```bash
export CARGO_TARGET_DIR=/tmp/assay-2176-durability-target
cargo test -p assay-cli --test agent_golden_path_contract
cargo test -p assay-mcp-server --test agent_golden_path_contract
cargo test -p assay-sim
cargo test -p assay-core agentic::tests
judge_list="$(cargo test -p assay-core judge_internal::tests::contract -- --list)"
judge_count="$(printf '%s\n' "$judge_list" | awk '/: test$/ { count++ } END { print count + 0 }')"
if [[ "$judge_count" -le 0 ]]; then
  echo "judge contract filter executed zero tests" >&2
  exit 1
fi
printf 'judge contract tests selected: %s\n' "$judge_count"
cargo test -p assay-core judge_internal::tests::contract
```

The simulation/agentic/judge runs are regression probes, not claims that this
script-only change alters model or runtime behavior.

- [ ] **Step 5: Run repository verification**

```bash
export CARGO_TARGET_DIR=/tmp/assay-2176-durability-target
cargo fmt --all -- --check
cargo clippy -p assay-cli -p assay-mcp-server -p assay-sim --all-targets -- -D warnings
git diff --check
git status --short
```

Verify the seven generated paths have no diff against
`55e8103029583a16af96e1eafe73025192d69cd2`.

- [ ] **Step 6: Push and open a draft PR**

```bash
git push --set-upstream origin codex/2176-golden-path-durability
gh pr create --draft \
  --base main \
  --head codex/2176-golden-path-durability \
  --title "test(agent): harden golden-path durability gates" \
  --body-file /tmp/assay-2176-pr-body.md
```

The PR body must include exact-head measurement provenance, named RED/GREEN
mutations, public-surface non-claims, the no-generated-byte assertion, and the
technical no-change disposition for Python toolchain centralization.

- [ ] **Step 7: Obtain final-head review quorum**

Use the existing context-bearing Claude Code Desktop review chat in read-only
mode and state the exact PR head SHA. Request CodeRabbit or Copilot review. If a
bot is unavailable under `AGENTS.md`, obtain a second non-building agent review
on the same SHA and record the substitution. A push after either review
invalidates it.

- [ ] **Step 8: Validate CI and prepare merge record**

Require all required checks green and no unresolved actionable thread. Measure
the external required-check list on the final head and record it rather than
claiming it is file-enforced. When the PR is ready, remove draft status, enable
auto-merge only after quorum, and add the exact branch, head, verification,
reviews, non-claims, and open PR-B notes to issue #2176.

## Deferred PR B Notes

The separate PR B plan must start only after PR A merges. It must carry these
already reviewed constraints:

- `working_directory` is present if and only if `working_directory_base` is
  present; `source_repo` is the only allowed base in v1.
- Generated skill wording names only the cwd referent; packaging or future
  plugin language remains forbidden public vocabulary.
- Missing `working_directory` renders as `invocation cwd`, not `.`.
- Re-verify the existing protected-action guide-row sed fixture in
  `scripts/ci/test-agent-golden-path-skill-hardening.sh`; it currently assumes
  the pre-PR-B `.` rendering for absent working directories.
- The protected-action fixture is source-repo-relative; #2152 step 4 must decide
  whether packaging carries or replaces that reference scenario.
- PR B rechecks and cites current official Cursor documentation before changing
  the compatibility wording.
