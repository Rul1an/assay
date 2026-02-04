# Pinned actions (SHA) — supply-chain hardening

Repo settings: **Allowed actions** should be restricted (e.g. "Allow GitHub-owned and verified creators") and **Require SHA pinning** enabled once workflows are pinned.

**Status:** ✅ All third-party actions are SHA-pinned (2026-01-30).

## Resolving SHAs

```bash
# Example: get latest commit SHA for a tag/branch
gh api repos/OWNER/REPO/commits/REF --jq .sha
```

## Pinned actions (current)

| Action | Original ref | Pinned SHA |
|--------|--------------|------------|
| `actions/checkout` | `@v4` | `34e114876b0b11c390a56381ad16ebd13914f8d5` |
| `actions/upload-artifact` | `@v4` | `ea165f8d65b6e75b540449e92b4886f43607fa02` |
| `actions/download-artifact` | `@v4` | `d3f86a106a0bac45b974a628896c90dbdf5c8093` |
| `actions/setup-python` | `@v5` | `a26af69be951a213d495a4c3e4e4022e16d87065` |
| `actions/cache` | `@v4` | `0057852bfaa89a56745cba8c7296529d2fc39830` |
| `dtolnay/rust-toolchain` | `@stable` | `4be9e76fd7c4901c61fb841f559994984270fce7` |
| `dtolnay/rust-toolchain` | `@nightly` | `881ba7bf39a41cda34ac9e123fb41b44ed08232f` |
| `Swatinem/rust-cache` | `@v2` | `779680da715d629ac1d338a641029a2f4372abb5` |
| `mozilla-actions/sccache-action` | `@v0.0.6` | `9e326ebed976843c9932b3aa0e021c6f50310eb4` |
| `bencherdev/bencher` | `@main` | `451ec1124b6d2c5797ac27d9a572233eb308e9d2` |
| `softprops/action-gh-release` | `@v2` | `a06a81a03ee405af7f2048a818ed3f03bbf83c7b` |
| `github/codeql-action/upload-sarif` | `@v3` | `439137e1b50c27ba9e2f9befc93e43091b449c34` |
| `dorny/test-reporter` | `@v1` | `d61b558e8df85cb60d09ca3e5b09653b4477cea7` |
| `rust-lang/crates-io-auth-action` | `@v1` | `b7e9a28eded4986ec6b1fa40eeee8f8f165559ec` |
| `PyO3/maturin-action` | `@v1` | `86b9d133d34bc1b40018696f782949dac11bd380` |
| `pypa/gh-action-pypi-publish` | `@release/v1` | `ed0c53931b1dc9bd32cbe73a98c7f6766f8a527e` |

## Dependabot for SHA updates

In `.github/dependabot.yml`:

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

Dependabot will propose PRs to update action refs; with SHA pinning, it will propose SHA bumps when the action repo has new commits on the same tag.

## Updating SHAs

When updating SHAs manually:

1. Resolve new SHA: `gh api repos/OWNER/REPO/commits/REF --jq .sha`
2. Update workflow files: `sed -i '' 's|OLD_SHA|NEW_SHA|g' .github/workflows/*.yml`
3. Update this document with the new SHA
4. Commit with message: `chore(ci): pin OWNER/REPO to SHA (was vX)`

## Security benefits

- **Immutable:** SHA ensures exact code version runs, even if tag is moved
- **Audit trail:** PRs show exactly which code changed
- **Supply chain:** Protects against tag hijacking or compromised releases
