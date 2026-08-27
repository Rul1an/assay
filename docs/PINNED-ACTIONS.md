# Pinned actions (SHA) — supply-chain hardening

Repo settings: **Allowed actions** should be restricted (e.g. "Allow GitHub-owned and verified creators") and **Require SHA pinning** enabled once workflows are pinned.

## Source of truth

The canonical pin for a third-party action is the `uses:` callsite in the YAML
that invokes it. This page does not restate those SHAs or tag refs. A duplicated
table was a second literal that Dependabot never updated and that drifted from
the callsites (see #2223).

`Rul1an/assay-action` is the exception. `.github/assay-action-pin` is the
execution authority for that published Action. Workflow `uses:` SHA literals
for it are parity-bound to the pin file; they are not a second place to change
the consumed commit. This page still does not restate the SHA.

Safe rollback is Assay-side only: restore the pin file and the parity-bound
`uses:` literals to an earlier immutable commit. Do not move floating `v3`.
Do not move frozen `v2`.

Callsite surfaces in this repository include:

- workflow files under `.github/workflows/**/*.yml`
- composite/action manifests such as `.github/actions/**/action.yml`

A surface can be empty of external `uses:` today (for example
`.github/actions/**` may have none) and still belongs in the scan set: new
callsites land there without this page being updated.

To inspect the pins currently in use across those surfaces:

```bash
rg -n --pcre2 'uses:\s*[^\s]+@[0-9a-f]{40}' \
  .github/workflows \
  .github/actions
```

Nothing in this repository regenerates or verifies this page against the
callsite YAML. Treat third-party `uses:` callsites as authoritative. For
`Rul1an/assay-action`, treat `.github/assay-action-pin` as authoritative and
the workflow SHA literals as parity-bound.

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

Dependabot proposes PRs that update action refs in callsite YAML it covers.
With SHA pinning, those PRs bump the SHA when the action repo advances on the
same tag. Dependabot does not maintain this document.

## Updating SHAs

When updating SHAs manually:

1. Resolve new SHA: `gh api repos/OWNER/REPO/commits/REF --jq .sha`
2. For third-party actions, update every callsite YAML that invokes the action
   (workflows under `.github/workflows/`, plus composite/action manifests such
   as `.github/actions/**/action.yml`). For `Rul1an/assay-action`, change
   `.github/assay-action-pin` first; then copy that SHA into the parity-bound
   `uses:` literals.
3. Commit with message: `chore(ci): pin OWNER/REPO to SHA (was vX)`

Do not add the SHA back into this document.

## Security benefits

- **Immutable:** SHA ensures exact code version runs, even if tag is moved
- **Audit trail:** PRs show exactly which code changed
- **Supply chain:** Protects against tag hijacking or compromised releases
