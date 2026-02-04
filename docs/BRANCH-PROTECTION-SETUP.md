# Branch protection & ruleset (main) — setup

Main is unprotected until you configure branch protection or a ruleset. This doc gives minimal SOTA 2026 settings and how to apply them (UI or `gh` CLI).

**Why:** Without protection, anyone with push access can push directly to `main`, bypassing CI, reviews, and status checks.

---

## Minimal settings (SOTA 2026)

- **Require a pull request** before merging (no direct pushes to main).
- **Required approvals:** at least 1–2.
- **Required status checks:** CI only (see "Required checks: when each is needed" for rationale; optional: Smoke Install, assay-action-contract-tests, MCP Security, Kernel Matrix).
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

**Canonical context names** (use these exactly; they match `name:` in the workflow files):

| Context | Workflow file |
|---------|----------------|
| `CI` | `.github/workflows/ci.yml` |
| `Smoke Install (E2E)` | `.github/workflows/smoke-install.yml` |
| `assay-action-contract-tests` | `.github/workflows/action-tests.yml` |
| `MCP Security (Assay)` | `.github/workflows/assay-security.yml` |
| `Kernel Matrix CI` | `.github/workflows/kernel-matrix.yml` |
| `Assay Gate` | `.github/workflows/assay.yml` |

Use **`CI`** (not `CIExpected` or any other variant). No workflow in this repo reports a check named `CIExpected`.

---

## Required checks: when each is needed

| Check | What it does | **Dependabot / deps-only PRs** | **Other PRs (features, workflows, action)** |
|-------|----------------|---------------------------------|---------------------------------------------|
| **CI** | Build, test, clippy, cargo-deny, cargo-audit, eBPF smoke | **Essential** — new deps must not break build or tests. | **Essential** — same. |
| **Smoke Install (E2E)** | Build from source, run assay, JUnit | Redundant with CI (CI already builds and tests). | Useful — verifies install path. |
| **assay-action-contract-tests** | Tests GitHub Action in `assay-action/` | Not needed — Cargo.toml/Cargo.lock don't touch the action. | **Essential** if PR touches `assay-action/` or workflows. |
| **MCP Security (Assay)** | Install assay, run validate with demo config | Redundant with CI for deps-only (CI validates the binary). | Useful — sanity check for security workflow. |
| **Kernel Matrix CI** | eBPF tests on self-hosted runner | Not needed — kernel-matrix `paths` exclude Cargo.toml/Cargo.lock. | **Essential** if PR touches eBPF/Monitor/evidence. |

**Recommendation:** Require **only CI** for merging. That is enough for Dependabot (deps-only) PRs and keeps the gate meaningful for all PRs. Smoke Install, assay-action-contract-tests, and MCP Security still run and appear on the PR; they are not required to merge. If you merge changes to `assay-action/` or workflows, ensure contract tests and MCP Security have passed before merging (e.g. via review policy or by re-adding them to required checks when needed).

---

## Option B: `gh` CLI (branch protection)

Classic branch protection via API (no rulesets). The API expects a **JSON body** with real booleans; form fields (`-f`) can send strings and cause 422 "Validation Failed". Use `--input -` with a JSON payload below.

**User-owned repos:** Do not send `restrictions` with users/teams or `dismissal_restrictions` with users/teams (only org repos support those). Use `restrictions: null` and omit or empty `dismissal_restrictions`.

```bash
# Replace OWNER/REPO (e.g. Rul1an/assay) and status check contexts to match your workflow job names.
gh api repos/OWNER/REPO/branches/main/protection -X PUT \
  -H "Accept: application/vnd.github+json" \
  -H "X-GitHub-Api-Version: 2022-11-28" \
  --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["CI"]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": {
    "dismiss_stale_reviews": false,
    "require_code_owner_reviews": true,
    "required_approving_review_count": 1
  },
  "restrictions": null,
  "allow_force_pushes": false,
  "allow_deletions": false
}
JSON
```

Use `required_approving_review_count: 2` in the JSON if you want two approvals. The listed contexts match this repo’s workflows (CI, Smoke Install, action tests, MCP Security); add e.g. `Kernel Matrix CI` if eBPF checks must be required.

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

- [x] Branch protection or ruleset on `main` with: require PR, approvals, status checks, up to date, no force-push.
- [x] Required status checks: CI only (see "Required checks: when each is needed" above; add Smoke Install / contract tests / MCP Security / Kernel Matrix when stricter gate needed).
- [x] CODEOWNERS in place; “Require review from Code Owners” enabled.
- [ ] Environments `release` / `crates` / `pypi` configured with required reviewers; release workflow uses `environment:` on publish jobs.

---

## Troubleshooting: "CIExpected — Waiting for status to be reported"

If a PR shows a required check **CIExpected** that never completes, branch protection is requiring a context that no workflow reports.

**Fix:** In **Settings → Branches → Branch protection rule for `main`**, under "Require status checks", remove **CIExpected** and add **CI** (from `.github/workflows/ci.yml`). Save.

**Via API:** Inspect with `gh api repos/OWNER/REPO/branches/main/protection`. Ensure `required_status_checks.contexts` contains `"CI"` and not `"CIExpected"`. Re-apply using the JSON in Option B above with `"contexts": ["CI"]`.
