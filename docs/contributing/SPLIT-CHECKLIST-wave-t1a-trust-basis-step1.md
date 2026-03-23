# Wave T1a Trust Basis Step1 Checklist

- [x] `trust-basis.json` is introduced as the canonical T1a compiler output.
- [x] The frozen v1 claim-key set is emitted in stable order and always fully present.
- [x] Claim classification happens only in the trust-basis/compiler stage.
- [x] The implementation stays scoreless and badge-less.
- [x] No `trustcard.json` or `trustcard.md` surface is introduced in T1a.
- [x] No new signals, packs, or engine semantics are introduced.
- [x] Canonical serialization stays deterministic, diff-friendly, and free of wall-clock or host-volatile fields.
- [x] Low-level CLI generation exists for advanced workflows without turning T1a into the Trust Card UX.
- [x] Artifact-first tests cover golden regeneration, conservative absent behavior, and explicit pack-execution behavior.
