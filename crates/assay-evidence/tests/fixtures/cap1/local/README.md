# CAP-1 local vectors

These vectors are local to Assay's CAP-1 test contract. They are not upstream vectors from
`Certisyn-Inc/certisyn-drafts`, and are kept under the `LC-*` prefix to avoid collisions with
the upstream `PV-*` and `NC-*` series.

- `LC-01.json` (formerly branch-local `NC-11`): schema-first rejection at `/strata/0/id` by using
  an uppercase stratum id (`"Detectors"`). This documents that schema validation runs before rules
  and does not duplicate any upstream vector's first failure.
- `generated:oversize` is not a committed file. Both crate tests build it at runtime from
  `normative/PV-01.json`, padded to one byte over `Cap1AdmissionLimits::HARD_MAX_BYTES`.
