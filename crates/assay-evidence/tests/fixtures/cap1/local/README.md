# CAP-1 local vectors

These vectors are local to Assay's CAP-1 test contract. They are not upstream vectors from
`Certisyn-Inc/certisyn-drafts`, and are kept under the `LC-*` prefix to avoid collisions with
the upstream `PV-*` and `NC-*` series.

- `LC-01.json` (formerly branch-local `NC-11`): schema-first rejection at `/strata/0/id` by using
  an uppercase stratum id (`"Detectors"`). This documents that schema validation runs before rules
  and does not duplicate any upstream vector's first failure.
- `LC-02.json`: profile const mismatch (`/profile`).
- `LC-03.json`: schema `minItems` guard for empty `strata` (`/strata`).
- `LC-04.json`: closed `basis.kind` vocabulary (`/strata/0/basis/kind`).
- `LC-05.json`: subject digest hex-pattern guard (`/subject/digest/value`).
- `LC-06.json`: basis catalogue digest hex-pattern guard (`/strata/0/basis/catalogue_digest`).
- `LC-07.json`: withheld digest hex-pattern guard (`/strata/0/unexamined/0/withheld_digest`).
- `LC-08.json`: root-level `additionalProperties: false` guard.
- `LC-09.json`: type guard for `eligible` (`/strata/0/eligible`).
- `LC-10.json`: minimum guard for `eligible` (`/strata/0/eligible`).
- `LC-11.json`: rules-stage R0 duplicate stratum id.
- `LC-12.json`: rules-stage R2 empty `unit`.
- `LC-13.json`: duplicate key (`profile`) with valid value first, then invalid.
- `LC-14.json`: duplicate key (`profile`) with invalid value first, then valid.
- `LC-15.json`: `examined > eligible`; first refusal is R1, and R5 is reached only when R1 is silenced.
- `generated:oversize` is not committed. Tests build it from `normative/PV-01.json`, padded to one
  byte over `Cap1AdmissionLimits::HARD_MAX_BYTES`.
- `generated:at-limit` is not committed. Tests build it from `normative/PV-01.json`, padded to
  exactly `Cap1AdmissionLimits::HARD_MAX_BYTES`.
- `generated:not-utf8` is not committed. Tests build a three-byte invalid UTF-8 payload.
- `generated:malformed-json` is not committed. Tests build a truncated JSON payload.
- `generated:depth-64` is not committed. Tests build a JSON payload nested 64 levels deep;
  admission accepts it and schema then refuses the non-CAP-1 shape.
- `generated:depth-65` is not committed. Tests build the same shape as `generated:depth-64` one
  level deeper (`[` * 65 + `0` + `]` * 65); admission refuses first on nesting depth (`>64`).
- `generated:order-depth-before-duplicate` is not committed. Tests build 64 array levels around an
  object carrying duplicate `k` keys; admission refuses first on nesting depth, pinning depth-before-duplicate order.
- `generated:keys-10000` is not committed. Tests build one object with exactly 10,000 keys;
  admission accepts it and schema then refuses the non-CAP-1 shape.
- `generated:keys-10001` is not committed. Tests build one object with 10,001 keys; admission
  refuses at the strict object-key ceiling.
- `generated:lone-surrogate` is not committed. Tests build a JSON string containing `\uD800`
  without a pairing low surrogate.
- `generated:bad-escape` is not committed. Tests build a JSON string containing `\u12G4`.
