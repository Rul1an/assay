# CAP-1 relying-party gate fixtures

Vectors and expected decisions for `assay_evidence::coverage_attestation`, pinned from the
Python reference `cap1-conforming-but-misleading` (public at
github.com/Rul1an/cap1-conforming-but-misleading) at its 2026-09-07 run. `vectors/` are the
20 original documents (6 MV misleading, 6 HV honest twins, 8 EV evasions); `upstream/` are
the five positive vectors of CAP-1 itself (Apache-2.0, copyright 2026 Certisyn, Inc.,
`Certisyn-Inc/certisyn-drafts@0980d32`); `context.json` is the relying party's own state;
`expected.json` is the decision table the Rust gate must reproduce.
