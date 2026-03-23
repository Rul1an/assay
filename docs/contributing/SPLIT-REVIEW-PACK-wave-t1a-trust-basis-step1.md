# Wave T1a Trust Basis Step1 Review Pack

## Intent

Ship the smallest possible implementation of the T1a compiler contract:

- verified bundle in
- canonical `trust-basis.json` out
- fixed claim vocabulary
- deterministic serialization
- no Trust Card rendering

## Reviewer focus

- Do all six frozen claims always exist, even when `absent`?
- Are `source` and `boundary` limited to the frozen vocabularies?
- Does any claim classification depend on raw OTel or upstream protocol material?
- Do signing/provenance stay conservative instead of opportunistically upgrading?
- Does the CLI stay low-level and artifact-first rather than becoming a Trust Card surface?

## Red flags

- any `trustcard.` files or rendering logic
- aggregate scores, badges, or `trusted/untrusted` output
- new signals, packs, or engine semantics
- non-deterministic serialization
- claims inferred from markdown, prose, or raw upstream formats
