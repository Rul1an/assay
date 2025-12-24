# Rollout Template (v0.3.4)

## Subject: Assay v0.3.4 (Adoption Hardening) Available

Hi Team,

We have released **Assay v0.3.4**, which focuses on "Adoption Hardening" — making the gate robust, easy to debug, and "green" by default in CI.

### 🚀 Upgrade Instructions
Update your GitHub Action to use **v0.3.4**:
```yaml
- uses: Rul1an/assay-action@v1 # or @v0.3.4
  with:
    assay_version: v0.3.4
    # ... other inputs ...
```

### ✨ What’s New
1.  **Split Caches**: No more "it works locally but fails in CI" due to cache collisions.
2.  **`assay validate`**: Preflight check for your config and traces.
3.  **`assay doctor`**: Generates a support bundle (`doctor.json`) if you get stuck.
4.  **Auto Fork Support**: Automatically handles permissions for fork PRs.

### 🛠️ Golden Path (How to Debug)
If your pipeline fails, follow these steps *before* asking for help:

1.  **Local Check**: Run `assay validate` in your repo.
2.  **Diagnostics**: If CI fails, the logs now show actionable errors (e.g. `E_TRACE_MISS`).
3.  **Support**: If you can't fix it, run:
    ```bash
    assay doctor --config eval.yaml --format json --out doctor.json
    ```
    ...and attach `doctor.json` to a ticket using the **[Design Partner Triage]** template.

### 📊 Feedback
We are tracking the "Top 10 Failure Modes" to improve the tool. Please report any friction you encounter!

---
*Release Notes: https://github.com/Rul1an/assay/releases/tag/v0.3.4*
