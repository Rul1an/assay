# CAP-1 relying-party gate fixtures

Vectors and expected decisions for `assay_evidence::coverage_attestation`, pinned from the
Python reference `cap1-conforming-but-misleading` (public at
github.com/Rul1an/cap1-conforming-but-misleading) at its 2026-09-07 run. `vectors/` are the
20 original documents (6 MV misleading, 6 HV honest twins, 8 EV evasions); `upstream/` are
the five positive vectors of CAP-1 itself (Apache-2.0, copyright 2026 Certisyn, Inc.,
`Certisyn-Inc/certisyn-drafts@0980d32`); `context.json` is the relying party's own state;
`expected.json` retains the historical Python decision table unchanged. The Rust test checks
all three claim-kind decisions for the 20 original documents, and rules fired for
bounded-negative claims only. It explicitly records these Assay deviations:

- EV-01b remains degraded but fires C5 as well as C1: its unknown catalogue digest has no
  relying-party unit set. Removing C1 alone therefore no longer allows it.
- PV-01 becomes degraded and fires C5 for its unavailable population.
- PV-02 through PV-05 retain their historical decisions and additionally fire C5.

The test owns these stricter expectations; they are not results of a new Python run. Fixture
and historical expectation bytes are preserved. The gate does not re-check CAP-1 conformance.

The retained reference output is [`runs/run.json`](https://github.com/Rul1an/cap1-conforming-but-misleading/blob/bbfe9f9b5c0a1d23600a8767ff6b0b5e28b57ddd/runs/run.json)
at commit `bbfe9f9b5c0a1d23600a8767ff6b0b5e28b57ddd`, SHA-256
`1bae6310f66d932751abc4c93865861c1f79a448fa95a62739fb0bf6ea6360ad`.
The 20 original and five upstream historical expectation rows match its stored output. This
pins retained bytes; it is not an independent rerun or authentication of the reference producer.
