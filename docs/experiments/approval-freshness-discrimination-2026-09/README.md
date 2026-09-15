# One approval-freshness mutation, four tests, two observations

> Experimental record. It shows that four existing tests in `assay-core` detect one specified
> change to the approval-freshness check. It does not show that the check is right, that the
> tests' expected results are right, or anything about other changes to the same code.

Everything a reader needs to redo the measurement is in this directory: the baseline commit, the
mutation as a patch, the four test names, the command, both outputs, and a script that reruns it and
compares. Nothing here depends on a branch name or on a commit that is not on `main`.

## Subject

`Rul1an/assay` at commit `e8f3a2c24172bdab0cc369bb61a03f2b78d7d4a1` (on `main`), toolchain as pinned
by the repository's `rust-toolchain.toml` at that commit (Rust 1.96.0).

The code under test is `validate_approval_required` in
`crates/assay-core/src/mcp/tool_call_handler/evaluate_next/approval.rs`. When a policy requires an
approval, it parses the approval artifact, classifies its freshness, and returns a deny with
`ApprovalFailure::ExpiredApproval` when the classification is not `Fresh`. After that it checks the
bound tool and the bound resource.

## The mutation

`mutant.patch` removes the early return on a non-fresh approval and nothing else. One file, one
insertion, six deletions. Parsing, the freshness classification (still written to
`tool_match.approval_freshness`), the bound-tool check and the bound-resource check are untouched.
The patch was generated with `git diff` against the baseline commit; the single trailing space on
its two blank context lines was removed (this repository's whitespace hook strips it anyway, and
`git apply` reads an empty hunk line as blank context). It applies cleanly with `git apply`; the
tree it produces is pinned below.

## The corpus

Four tests that already existed at the baseline commit, in
`crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs`, selected by the filter
`mcp::tool_call_handler::tests::approval` and not modified for this record:

- `approval_required_missing_denies`
- `approval_required_expired_denies`
- `approval_required_bound_tool_mismatch_denies`
- `approval_required_bound_resource_mismatch_denies`

The expired case uses the fixture `approval_artifact(tool, resource, -30)`: `issued_at` five minutes
in the past, `expires_at` thirty seconds in the past, both wall-clock.

Command, identical for both runs, executed in a detached `git worktree` with its own
`CARGO_TARGET_DIR`:

```bash
cargo test --locked -p assay-core --lib mcp::tool_call_handler::tests::approval -- --test-threads=1
```

## Two observations

| Run | Tree | Summary line (verbatim, wall-clock suffix dropped) | Exit |
|---|---|---|---:|
| baseline: `e8f3a2c2…` unmodified | `141b8b49f2d6e33d77ae6e15a348f22ec1ff0feb` | `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 977 filtered out` | 0 |
| mutant: `e8f3a2c2…` + `mutant.patch` | `b9783c281c8c1e2d6622461fb27a027b6f487c7a` | `test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 977 filtered out` | 101 |

The one failing test is `approval_required_expired_denies`. Its assertion at `tests/approval.rs:111`
reports `expected deny result, got Allow { … }`. Inside the printed decision event the handler still
records `approval_freshness: Some(Expired)` next to `decision: Allow`: the classification survived,
only the rejection was gone. The mutant build also emits a dead-code warning, `variant
ExpiredApproval is never constructed`, because the removed branch was that variant's only
constructor.

The full outputs are `observations/baseline.txt` and `observations/mutant.txt`. Each starts with a
header naming the tree, worktree state, toolchain, command, UTC start and end, and exit status, and
saying exactly what was trimmed: compile-progress lines (`Downloading`, `Downloaded`, `Compiling`)
and, for the baseline, one trailing blank line. From the `Finished` line onward the text is
verbatim, including the mutant's compiler warning and the failure section. The panic line carries
run-specific values (an event id, timestamps); those are expected to differ between runs, which is
why the comparison below uses result lines only.

## Recompute

```bash
docs/experiments/approval-freshness-discrimination-2026-09/recompute.sh
```

From any checkout of the repository that contains the baseline commit. The script checks the record
files against `SHA256SUMS.txt`, adds a detached worktree at the baseline commit, runs the command,
applies `mutant.patch` with `git apply --index`, checks that `git write-tree` yields the pinned tree,
runs the command again, and compares the result lines of each run (`test … ... ok|FAILED` and
`test result:`, minus `finished in …`) with the observation files. Any mismatch, including an
unexpected exit status, exits non-zero. The worktree and its `target/` are removed on exit. The
script itself contacts nothing; `cargo` resolves crates from `Cargo.lock` and may fetch them from the
registry if they are not cached locally. Expect two builds of the `assay-core` test profile (about
three minutes and a few GB of `target/` on an Apple M-series machine).

Run record, 2026-09-15, by the author of this record, on the same machine as the observations
(Rust 1.96.0, aarch64-apple-darwin), from the worktree holding this directory:
`./recompute.sh` on the committed bytes started 19:28:40Z, finished 19:30:33Z, exit 0, last line
`recompute: both observations reproduced`. A control on a scratch copy of this directory with the
baseline summary line edited to `3 passed; 1 failed` (and `SHA256SUMS.txt` regenerated to match)
exited 1 at `baseline: result lines differ from observations/baseline.txt`, so the comparison is
live rather than passing by construction.

By hand, the same thing is:

```bash
git worktree add --detach /tmp/afd e8f3a2c24172bdab0cc369bb61a03f2b78d7d4a1
cd /tmp/afd && cargo test --locked -p assay-core --lib mcp::tool_call_handler::tests::approval -- --test-threads=1
git apply --index <this dir>/mutant.patch && git write-tree   # b9783c28…
cargo test --locked -p assay-core --lib mcp::tool_call_handler::tests::approval -- --test-threads=1
```

## What this shows, and what it does not

Shows: with the four tests held fixed, the baseline tree and the patched tree are distinguishable,
the distinguishing test is the expiry test, and the difference is a handler decision (Deny on the
baseline, Allow on the mutant), not a build failure or a changed diagnostic.

Does not show:

- that the freshness rule, or the expected result the test asserts, is correct. The oracle is the
  pre-existing assertion; nothing here reviews it independently.
- anything about mutations other than this one, or about the other three checks in the same
  function. One mutation was specified and measured; no survivor set, no coverage figure.
- boundary behaviour. The fixture is thirty seconds past expiry; the exact boundary, clock skew,
  revocation, and check-then-use ordering are not exercised.
- anything about a deployed system, an approver's identity, or a call that reached a real tool. The
  inputs are synthetic requests handled in-process by the production code path.

## Digests

Commit and tree pins are git object ids; file digests are SHA-256 over the committed bytes and are
also in `SHA256SUMS.txt` (`shasum -a 256 -c SHA256SUMS.txt`).

| Object | Id / SHA-256 |
|---|---|
| baseline commit | `e8f3a2c24172bdab0cc369bb61a03f2b78d7d4a1` |
| baseline tree (`e8f3a2c2^{tree}`) | `141b8b49f2d6e33d77ae6e15a348f22ec1ff0feb` |
| patched tree (`git apply --index mutant.patch && git write-tree`) | `b9783c281c8c1e2d6622461fb27a027b6f487c7a` |
| `mutant.patch` | `662324e39a24a8659529a54930ea962239c71a6fe266e74b5ea76aeaa140de74` |
| `observations/baseline.txt` | `e8ec316404c92bced962b8cb4a15f96ea455d6f5d039281a71c689741de36b40` |
| `observations/mutant.txt` | `491d7b239e041645886b8342fb16805c97dae24b12c19c98647cc3a996a4401b` |
| `recompute.sh` | `86f53c4c4d610395063ef9ad2d35d718027f171e39416bfdd4bfe824987dba48` |
