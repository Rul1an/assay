# Pinned actions (SHA) — supply-chain hardening

Repo settings: **Allowed actions** should be restricted (e.g. "Allow GitHub-owned and verified creators") and **Require SHA pinning** enabled once workflows are pinned.

## Source of truth

Workflow files under `.github/workflows/` are the only source of truth for
which third-party actions are used and which commit SHAs pin them. This page
does not restate those SHAs or tag refs. A duplicated table was a second literal
that Dependabot never updated and that drifted from the workflows (see #2223).

To inspect the pins currently in use:

```bash
rg -n --pcre2 'uses:\s*[^\s]+@[0-9a-f]{40}' .github/workflows/
```

Nothing in this repository regenerates or verifies this page against the
workflows. Treat the workflows as authoritative; treat this page as procedure
only.

## Resolving SHAs

```bash
# Example: get latest commit SHA for a tag/branch
gh api repos/OWNER/REPO/commits/REF --jq .sha
```

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

Dependabot proposes PRs that update action refs in the workflow files. With SHA
pinning, those PRs bump the SHA when the action repo advances on the same tag.
Dependabot does not maintain this document.

## Updating SHAs

When updating SHAs manually:

1. Resolve new SHA: `gh api repos/OWNER/REPO/commits/REF --jq .sha`
2. Update the workflow files that call the action (for example:
   `sed -i '' 's|OLD_SHA|NEW_SHA|g' .github/workflows/*.yml`)
3. Commit with message: `chore(ci): pin OWNER/REPO to SHA (was vX)`

Do not add the SHA back into this document.

## Security benefits

- **Immutable:** SHA ensures exact code version runs, even if tag is moved
- **Audit trail:** PRs show exactly which code changed
- **Supply chain:** Protects against tag hijacking or compromised releases
