# CAP-1 duplicate-name probes

Five supplementary probes for the CAP-1 requirement to reject duplicate member names, copied
byte-for-byte from `Certisyn-Inc/certisyn-drafts@fd32187601038eee6afffc09923051c28efe6949`
(`cap-1/src/probes/duplicate-names/`, contributed by b7n0de in certisyn-drafts#11). They are
modified copies of `PV-01` and `PV-03`; `UPSTREAM-README.md` is the upstream modification
notice, kept verbatim, and `NOTICE` is `cap-1/NOTICE` at the same commit. Apache-2.0.
`SHA256SUMS` pins the bytes and `tests/cap1_duplicate_name_probes.rs` recomputes them.

Upstream's `verify.py` and `verify.mjs` accept S11, S13 and S14 and refuse S12 and S15 by a
rule rather than by the repeated name. The test here asserts that `verify_cap1_document`
refuses all five at strict syntax, and that the typed path behind that stage would have
accepted S11, S13 and S14, so the strict scan is the guard that refuses them.
