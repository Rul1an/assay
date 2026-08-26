# assay-action (non-authoritative)

This directory is **not** the published GitHub Action and is **not** Assay's
execution pin.

- Published action: https://github.com/Rul1an/assay-action
- Marketplace slug: `Rul1an/assay-action@v3`
- Consumer execution pin: `.github/assay-action-pin` (one 40-hex commit)
- Published `action.yml` bytes for that pin: `scripts/ci/fixtures/assay-action-pin/`

`resolve-version.sh` and `verify-install.sh` remain here as helpers for
Assay's release-channel tests. They do not define which Action commit CI
executes. Workflows must use `Rul1an/assay-action@<pin>`, not `./assay-action`.
