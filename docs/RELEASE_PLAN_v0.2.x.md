# Release & Baseline Hardening Plan (v0.2.x)

**Epic Goal**: "Design Partner Pilot Ready"
**Definition of Done**:
- [ ] PR gate works with `baseline.json` fetched from `origin/main`.
- [ ] Main job exports `baseline.json` as an artifact without errors.
- [ ] Tag `v0.2.1` publishes release assets (tarballs + checksums) to GitHub Releases.
- [ ] `verdict-action` installs successfully on Ubuntu & macOS runners.
- [ ] Docs contain copy/paste "Golden Path".

---

## PR13: verdict-action Polish (Baseline Inputs)

**Scope**: Make baseline workflow "first-class" in GitHub Action.

### Checklist
- [ ] **Inputs**: Add `baseline` and `export_baseline` to `action.yml`.
- [ ] **Forwarding**:
    - `baseline` -> `--baseline <path>`
    - `export_baseline` -> `--export-baseline <path>`
- [ ] **Artifacts**: Conditional upload of `export_baseline` file (only if exists).
- [ ] **Docs**: Updated `README.md` with Main (Export) vs PR (Gate) specific examples.

### Verification
- [ ] **Manual**: Run action with `baseline` input; verify CLI args.
- [ ] **CI (PR)**: Repo baseline -> Run OK.
- [ ] **CI (Main)**: Export baseline -> Artifact uploaded.

### Acceptance Criteria
- [ ] No upload-artifact failures on missing baseline.
- [ ] `baseline` input triggers gating logic.

---

## PR14: Core Hardening (Baseline Schema)

**Scope**: Prevent silent mismatches with strict validation.

### Checklist
- [ ] **Schema**: Add `config_fingerprint` field to Baseline struct.
- [ ] **Validation (Exit 2)**:
    - `schema_version` mismatch.
    - `suite` name mismatch.
- [ ] **Validation (Warn)**:
    - `verdict_version` mismatch.
    - `config_fingerprint` mismatch.
- [ ] **UX**: Actionable error messages ("Regenerate baseline using...").

### Verification
- [ ] **Test**: Schema mismatch -> Exit 2.
- [ ] **Test**: Suite mismatch -> Exit 2.
- [ ] **Test**: Version/Fingerprint mismatch -> Warning in run.json/stderr.

### Acceptance Criteria
- [ ] Config errors exit with code 2.
- [ ] Warnings are visible in logs.

---

## PR15: Release Workflow (Assets)

**Scope**: Automated binary distribution for reliable Action installation.

### Checklist
- [ ] **Trigger**: On tag `v*`.
- [ ] **Matrix**:
    - `ubuntu-latest` -> `verdict-linux-x86_64.tar.gz`
    - `macos-13` (x86) -> `verdict-macos-x86_64.tar.gz`
    - `macos-14` (arm64) -> `verdict-macos-aarch64.tar.gz`
- [ ] **Artifacts**: Tarballs contain `verdict` binary in root.
- [ ] **Publish**: Generate `checksums.txt` (sha256) and upload all to GitHub Release.

### Verification
- [ ] **Manual**: Push tag `v0.2.1-rc1`.
- [ ] **Check**: Assets appear in GitHub Releases.
- [ ] **Action**: Install step resolves URL and runs `verdict version`.

### Acceptance Criteria
- [ ] 3 assets + checksums present.
- [ ] Action works out-of-the-box.

---

## PR16: Documentation & Marketplace

**Scope**: Discoverability and Troubleshooting.

### Checklist
- [ ] **User Guide**: "3-step Baseline Workflow" (Export -> Fetch -> Gate).
- [ ] **Troubleshooting**:
    - "Baseline missing"
    - "Schema mismatch" (Exit 2)
    - "Fingerprint mismatch" (Warn)
- [ ] **Marketplace**: Manual publish step checked in GitHub Settings.

### Verification
- [ ] **Docs**: Examples are copy-paste executable.

### Acceptance Criteria
- [ ] Design partner can set up PR gating in < 15 mins.

---

## Rollout Plan
1. Merge **PR13**, **PR14**, **PR15**.
2. Push Tag `v0.2.1`.
3. Verify Release Assets & Action Install.
4. Manually publish to GitHub Marketplace.
5. Send Invite to Partners.
