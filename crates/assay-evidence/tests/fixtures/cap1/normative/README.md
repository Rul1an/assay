# CAP-1 normative vectors

The fifteen conformance vectors of CAP-1 (`PV-01`…`PV-05` conform, `NC-01`…`NC-10` refuse),
copied byte-for-byte from `Certisyn-Inc/certisyn-drafts@0980d3201aa2caab3cbad5c6e9bc99b422370b43`
(`cap-1/src/vectors/`). Apache-2.0, copyright 2026 Certisyn, Inc. `SHA256SUMS` pins the bytes
and `tests/cap1_normative_verify.rs` recomputes them.

The expected first failure per vector is stated in that test from the schema-first ordering of
SPEC-Incident-Package-v1 §6, not copied from the upstream run record, which lists every rule that
fires rather than the first. Two consequences the test names: `NC-02` is a schema rejection
before R2, and `NC-05` fails R1 before R5 is reached.

The sibling `upstream/` directory holds the five positive vectors as retained by the
relying-party gate fixtures (different whitespace, same content); the gate test owns those.
