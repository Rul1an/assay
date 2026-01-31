# Dependabot runs

Overview of Dependabot configuration and which workflows run on Dependabot PRs.

## Configuration

- **File:** [`.github/dependabot.yml`](../.github/dependabot.yml)
- **Schedule:** weekly on Monday for all ecosystems
- **Ecosystems:**
  - **github-actions** (directory `/`): SHA bumps for workflow actions. See [PINNED-ACTIONS.md](PINNED-ACTIONS.md).
  - **cargo** (directory `/`): dependency updates; patch updates ignored; see ignore rules below.
  - **pip** (directory `/assay-python-sdk`): Python SDK dependencies
- **Limits:** max 5 open PRs (actions, pip), max 10 (cargo)

## Ignore rules (Cargo)

| Dependency   | Update type   | Reason |
|-------------|---------------|--------|
| `*`         | semver-patch  | Less noise; patch updates manually if needed |
| `rand`      | semver-major  | See [ADR-020](architecture/ADR-020-Dependency-Governance.md); issue [#84](https://github.com/Rul1an/assay/issues/84) |
| `rand_core` | semver-major  | Same (rand 0.9 ecosystem) |
| `nix`       | semver-major  | Clippy ICE on 0.31; revisit when Clippy/nix updates |
| `aya-log-ebpf` | all        | Must stay in sync with aya-ebpf; bump both manually |

## Which workflows run on Dependabot PRs

Dependabot opens PRs from a branch in the **same repo** (not a fork). Therefore:

- **CI** (`ci.yml`): runs fully, unless only `docs/**`, `**.md` or `.gitignore` changed (paths-ignore). Includes Clippy, tests, perf (Criterion), and on same-repo PRs also **ebpf-smoke-self-hosted**.
- **Perf (PR compare)** (`perf_pr.yml`): runs (same-repo condition is true); compares with main baseline on Bencher.
- **assay-security**, **smoke-install**, **parity**, **baseline-gate-demo**, **kernel-matrix**: run on `pull_request`; no exception for Dependabot.

There is **no** `if: github.actor != 'dependabot[bot]'` in the workflows: dependency PRs get the same checks as regular PRs (including perf and self-hosted ebpf-smoke).

## Current open Dependabot PRs (Jan 2026)

| PR | Update | Status |
|----|--------|--------|
| **#79** | aya-log-ebpf aya-v0.13.0 → aya-v0.13.1 | **Closed:** two versions of `aya_ebpf` in dependency tree (PR bumped only aya-log-ebpf; aya-ebpf stayed on v0.13.0). Added Dependabot ignore for `aya-log-ebpf`. |
| **#76** | base64 0.21.7 → 0.22.1 | **Transient failure:** MCP Security workflow got `curl: (22) 403` from getassay.dev (rate-limit/infra), not caused by base64 update. Re-run or merge later. |
| #86, #83, #81, #78, #75, #73, #71, #70, #67 | jsonschema, crossterm, rust-toolchain SHA, procfs, dirs, thiserror, uuid, ratatui, criterion | Green or still running (e.g. eBPF jobs, Free disk). |

## Merging

**Note:** `@dependabot merge` is deprecated as of January 2026. Use GitHub's native controls.

### Enable auto-merge (one-time setup)

1. Go to **Settings → General → Pull Requests**
2. Check **"Allow auto-merge"**
3. Optionally check **"Automatically delete head branches"**

### Merge Dependabot PRs

```bash
# Enable auto-merge (merges when CI passes)
gh pr merge <number> --auto --squash

# Or merge immediately if CI is already green
gh pr merge <number> --squash

# Bulk approve and auto-merge all Dependabot PRs
for pr in $(gh pr list --author "app/dependabot" --state open --json number --jq '.[].number'); do
  gh pr review $pr --approve
  gh pr merge $pr --auto --squash
done
```

### Notes

- For SHA updates of actions: see [PINNED-ACTIONS.md](PINNED-ACTIONS.md) (verify new SHA is correct).
- For Cargo: after merge, `cargo update` locally or in a follow-up PR if desired.
- **aya (assay-ebpf):** both `aya-ebpf` and `aya-log-ebpf` must have the same git tag; Dependabot no longer opens PRs for aya-log-ebpf alone.
