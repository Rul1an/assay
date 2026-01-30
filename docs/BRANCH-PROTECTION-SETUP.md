# Branch protection & ruleset (main) — setup

Main is unprotected until you configure branch protection or a ruleset. This doc gives minimal SOTA 2026 settings and how to apply them (UI or `gh` CLI).

**Why:** Without protection, anyone with push access can push directly to `main`, bypassing CI, reviews, and status checks.

---

## Minimal settings (SOTA 2026)

- **Require a pull request** before merging (no direct pushes to main).
- **Required approvals:** at least 1–2.
- **Required status checks:** CI, Smoke Install (E2E), MCP Security (assay-security). Add Kernel Matrix if eBPF paths must be green.
- **Require branch to be up to date** before merging.
- **Restrict force-push and branch deletion** (do not allow force-push to main; restrict who can delete the branch).

**Extra (recommended):**

- **Require review from Code Owners** for:
  - `.github/workflows/**`
  - `release.yml` (if separate)
  - `assay-action/**`
  - `infra/**`

Ensure `.github/CODEOWNERS` exists and lists the right owners (see repo root).

---

## Option A: GitHub UI

1. **Settings → Code and automation → Rules → Rulesets** (or **Branches** for classic branch protection).
2. **Create rule** (or “Add rule” / “Add branch protection rule”).
3. **Target:** Branch rule, branch name pattern `main`.
4. Enable:
   - Require a pull request before merging.
   - Require approvals (set number, e.g. 1).
   - Require status checks: add `CI`, `Smoke Install (E2E)`, `assay-action-contract-tests` (or the exact job names your workflows expose; check **Settings → Branches → Branch protection** or the Ruleset UI for the list of available checks).
   - Require branches to be up to date before merging.
   - Do not allow force pushes / restrict force pushes.
   - Restrict who can push to matching branches (optional; or leave as default).
5. If using **Code Owners:** enable “Require a review from Code Owners” and ensure CODEOWNERS covers the paths above.

**Note:** Exact status check names come from your workflow `name` and job `name` (or job id). After the first run on a PR, they appear in the “Status checks that are required” dropdown.

---

## Option B: `gh` CLI (branch protection)

Classic branch protection via API (no rulesets).

```bash
# Required status checks: use the exact names from your workflows (e.g. "CI", "Smoke Install (E2E)")
gh api repos/OWNER/REPO/branches/main/protection \
  -X PUT \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks='{"strict":true,"contexts":["CI","Smoke Install (E2E)","assay-action-contract-tests"]}' \
  -f enforce_admins=false \
  -f required_pull_request_reviews='{"dismissal_restrictions":{},"dismiss_stale_reviews":false,"require_code_owner_reviews":true,"required_approving_review_count":1}' \
  -f restrictions=null \
  -f allow_force_pushes=false \
  -f allow_deletions=false
```

Replace `OWNER/REPO` (e.g. `Rul1an/assay`) and adjust `contexts` to match your workflow/job names. Use `required_approving_review_count: 2` if you want two approvals.

To **inspect** current protection:

```bash
gh api repos/OWNER/REPO/branches/main/protection
```

To **remove** protection (use with care):

```bash
gh api repos/OWNER/REPO/branches/main/protection -X DELETE
```

---

## Environments (release / publish gates)

For human-in-the-loop on release and publish:

1. **Settings → Environments** → create (if missing):
   - `release` — for the “Create Release” job and/or release workflow.
   - `crates` — for publish to crates.io.
   - `pypi` — already exists; use for publish to PyPI.
2. For each environment, add **Required reviewers** (e.g. 1–2 maintainers).
3. In `release.yml`, set `environment: release` (or `crates` / `pypi`) on the corresponding jobs so that runs wait for approval before executing.

See `docs/REVIEWER-PACK.md` (sectie 3, “Environments & approvals”) and the current `release.yml` for which jobs already use `environment:`.

---

## Checklist

- [ ] Branch protection or ruleset on `main` with: require PR, approvals, status checks, up to date, no force-push.
- [ ] Required status checks include: CI, Smoke Install (E2E), MCP Security (and Kernel Matrix if needed).
- [ ] CODEOWNERS in place; “Require review from Code Owners” enabled if desired.
- [ ] Environments `release` / `crates` / `pypi` configured with required reviewers; release workflow uses `environment:` on publish jobs.
