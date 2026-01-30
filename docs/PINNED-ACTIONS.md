# Pinned actions (SHA) — supply-chain hardening

Repo settings: **Allowed actions** should be restricted (e.g. “Allow GitHub-owned and verified creators”) and **Require SHA pinning** enabled once workflows are pinned.

This file is a **template** for high-risk actions: replace `@vX` / `@main` with `@<sha>` and use Dependabot (or Renovate) to bump SHAs periodically.

## Resolving SHAs

```bash
# Example: get latest commit SHA for a tag/branch
gh api repos/OWNER/REPO/commits/REF --jq .sha
```

## High-risk (pin first) — phased

| Action | Current ref | Resolved SHA (example) | Used in |
|--------|-------------|------------------------|---------|
| `bencherdev/bencher` | `@main` | `451ec1124b6d2c5797ac27d9a572233eb308e9d2` | perf_main.yml, perf_pr.yml |
| `softprops/action-gh-release` | `@v2` | `a06a81a03ee405af7f2048a818ed3f03bbf83c7b` | release.yml |
| `github/codeql-action/upload-sarif` | `@v4` | `b20883b0cd1f46c72ae0ba6d1090936928f9fa30` | assay-security.yml |
| `dorny/test-reporter` | `@v1` | `d61b558e8df85cb60d09ca3e5b09653b4477cea7` | smoke-install.yml |
| `actions/checkout` | `@v4` | `34e114876b0b11c390a56381ad16ebd13914f8d5` | all workflows |
| `dtolnay/rust-toolchain` | `@stable` | (use tag `stable` → SHA via API) | ci, release, parity, etc. |

**Note:** SHAs above are examples from a single run of `gh api repos/.../commits/REF`. Re-resolve before pinning; tags like `v4` move. Prefer Dependabot to bump SHAs so you get PRs when actions release new versions.

## Example YAML change (Bencher)

Before:

```yaml
- uses: bencherdev/bencher@main
```

After (phased):

```yaml
- uses: bencherdev/bencher@451ec1124b6d2c5797ac27d9a572233eb308e9d2
```

## Dependabot (optional)

In `.github/dependabot.yml` you can add:

```yaml
version: 2
updates:
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    commit-message:
      prefix: "chore(ci)"
```

Dependabot will propose PRs to update action refs; once you switch to SHA pinning, it will propose SHA bumps when the action repo has new commits on the same tag.

## Strict vs phased

- **Phased (recommended):** Pin Bencher, gh-release, upload-sarif, test-reporter first; then add top-level `permissions: {}` and environment gates; then pin the rest.
- **Strict:** Pin every third-party action to SHA in one go (larger diff; do after testing).
