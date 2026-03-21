# Wave S2 Move Map

## Shared Inventory

- `scripts/ci/release_archive_inventory.sh`
  - canonical archive inventory for S1 and S2

## Existing Flow Reused

- `scripts/ci/release_attestation_enforce.sh`
  - now imports the shared release archive inventory

## New Flow

- `scripts/ci/release_proof_kit_build.sh`
  - downloads attestation bundles
  - snapshots trusted root
  - copies release provenance summary
  - derives `manifest.json`
  - emits offline/online helper scripts
  - tars the proof kit only after full success
