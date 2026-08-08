#!/usr/bin/env bash
# The generated docs must match what the generators produce (#2080).
#
# `docs-auto-update.yml` used to detect this drift after a merge and open a PR from
# `docs/auto-update`. Those PRs cannot be checked: `create-pull-request` signs them with the default
# `GITHUB_TOKEN`, and GitHub does not run workflows for events raised by that token -- an
# anti-recursion safeguard. Fifteen `pull_request` runs on #2021 sat at `action_required` and none
# executed, so `CI` and `host-capability-check` never reported and the PR was blocked forever.
#
# Dispatching those workflows by hand looks like a fix and is not: every substantive job is
# conditioned on pull-request context, so a `workflow_dispatch` run of CI skips its whole matrix and
# reports success having examined nothing.
#
# GitHub's own remedy is a GitHub App installation token, which does trigger workflows. That keeps
# the bot-PR flow and needs an App plus two secrets. This takes the other route, and it is the one
# this repository already committed to: the drift is *checked* on the change that causes it rather
# than auto-committed afterwards. Six sibling checks work this way -- the gating map, the ADR-045
# seal fixtures, the release surface, the CI gate expectation, the MCP registry transaction, the
# fuzz lane -- and each of them exists because a generated artifact and its generator drift apart
# silently.
#
# The practical difference: the diagram edge lands in the pull request that added the dependency,
# where a reviewer can see *why* it changed, instead of arriving hours later in a PR nobody can
# merge.
#
# The generators write in place, so this copies the worktree to a scratch directory and runs them
# there. `check-aee-seal-fixture-drift.sh` carries the same warning for the same reason: an earlier
# version of that script ran its emitter in place and destroyed uncommitted edits in the very
# fixtures it was auditing.

set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

GENERATORS=(
  scripts/docs/generate-agent-golden-path.py
  scripts/docs/generate-crate-deps.sh
  scripts/docs/generate-module-map.sh
  scripts/docs/update-architecture-docs.sh
)

# The files the generators own. Anything outside this list is not compared, so a hand-written doc
# living beside a generated one is not held to the generator's output.
GENERATED=(
  docs/generated/agent-golden-path.json
  docs/generated/crate-deps.mermaid
  docs/generated/module-map.mermaid
  docs/AIcontext/architecture-diagrams.md
  docs/guides/agent-golden-path.md
)

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

# Tracked files as they are *now*, not as they are at HEAD.
#
# `git archive HEAD` was the first version and is wrong for the case that matters: at pre-commit
# time HEAD is the previous commit, so the generators would run against source that predates the
# change being committed and the check would pass on exactly the change it exists to catch.
# `git ls-files` takes the working-tree contents of tracked files, which is the state under review.
#
# Tracked-only, so a stray `target/` or an untracked scratch file cannot change what the generators
# see — and so the copy stays small.
mkdir -p "$scratch/repo"
git ls-files -z | tar -cf - --null -T - | (cd "$scratch/repo" && tar -xf -)

for generator in "${GENERATORS[@]}"; do
  if [[ "$generator" == *.py ]]; then
    runner=(python3)
  else
    runner=(bash)
  fi
  if ! (cd "$scratch/repo" && "${runner[@]}" "$generator" >/dev/null 2>&1); then
    echo "error: $generator failed inside the scratch copy, so no comparison was made." >&2
    echo "       This is a 'could not check', not a pass." >&2
    exit 1
  fi
done

drifted=()
for path in "${GENERATED[@]}"; do
  if [ ! -f "$scratch/repo/$path" ]; then
    echo "error: the generators did not produce $path." >&2
    exit 1
  fi
  if ! diff -q "$path" "$scratch/repo/$path" >/dev/null 2>&1; then
    drifted+=("$path")
  fi
done

if [ ${#drifted[@]} -gt 0 ]; then
  echo "error: generated docs do not match their generators:" >&2
  for path in "${drifted[@]}"; do
    echo >&2
    diff -u "$path" "$scratch/repo/$path" | sed -n '1,40p' >&2
  done
  echo >&2
  echo "Regenerate and commit:" >&2
  for generator in "${GENERATORS[@]}"; do
    if [[ "$generator" == *.py ]]; then
      echo "  python3 $generator" >&2
    else
      echo "  bash $generator" >&2
    fi
  done
  echo >&2
  echo "The diagram belongs in the change that moved it. A dependency edge added in one PR and" >&2
  echo "documented in another is a diagram nobody reviewed against the code it describes." >&2
  exit 1
fi

echo "generated docs reproduce from their generators (${#GENERATED[@]} files)."
