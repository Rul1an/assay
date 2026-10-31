# Deferred Work

Work Assay decided not to schedule, what stands instead, and what would reopen it. Closing an issue
here means nobody is building it now. It does not mean the work is done, and the listed limits still
apply. Each linked issue keeps its measurements, reasoning and history.

A trigger is checked when someone reads this page or works in that area. Nothing watches for it, so a
trigger can fire unnoticed. If yours has fired, reopen the issue and say what you observed.

| Work | Where it stands | Reopen when | Issue |
|---|---|---|---|
| IPv6, UDP/QUIC, DNS, already-open sockets, raw sockets and proxy identity enforcement | Outside the enforcement claim. IPv4 TCP `connect()` is enforced, and IPv6 CIDR policies are refused. See the [runtime monitor guide](../guides/runtime-monitor.md). | A delegated proof shows non-TCP/IPv4 egress, a consumer asks for it, or a release or document would widen the egress claim. | [#1576](https://github.com/Rul1an/assay/issues/1576) |
| AEE corpus re-run | Not run. The blind 179 of 232 at revision 22 was never committed as its own build, so nobody can reproduce it, and it is not a baseline for a later revision. | An upstream suite revision becomes a release claim, or an ADR-043 boundary decision depends on it. The next run commits the blind build first and pins suite revision, build ref and toolchain. | [#2132](https://github.com/Rul1an/assay/issues/2132) |
| Migrating CLI JSON schema families | Named identities are shipped, and existing readers keep their current forms. See the [identity inventory](../architecture/CLI-JSON-IDENTITIES.md). | A concrete producer or consumer change creates ambiguity or blocks compatibility. Migrate that one family, with expand/migrate/contract and old and new fixtures. | [#2485](https://github.com/Rul1an/assay/issues/2485) |
| Evidence retention, deletion and an S3 object-lock probe | The model is specified and not built: Assay issues no deletes of canonical objects, and recorded retention state is the design. S3 reports `unobserved:not_probed`. | A deployment needs retention under Assay's control or relies on object-lock state. The probe needs its own environment admission. | [#2492](https://github.com/Rul1an/assay/issues/2492) |
| A separate minimal native verifier | The published CLI verifies offline. A second binary must not duplicate it. | A recipient cannot install or trust the CLI, and says why. | [#2498](https://github.com/Rul1an/assay/issues/2498) |
| A non-Rust verifier | Not built. A binding would not be independent reproduction, which stays in [#1840](https://github.com/Rul1an/assay/issues/1840). | A named non-Rust consumer. | [#2499](https://github.com/Rul1an/assay/issues/2499) |
| A preserved fragmented-IPI re-run | The harness stays canonical. The artifacts behind the 2026Q1 results were not preserved. | A product or release decision depends on a fresh result, with named artifact storage and a run budget. | [#2519](https://github.com/Rul1an/assay/issues/2519) |
| A second historical retention record | The v5.3.0 to v5.4.0 record stays fixed. No later pair is recorded. | A concrete consumer or release claim needs a pair. Choose the pair then. | [#2717](https://github.com/Rul1an/assay/issues/2717) |
| A native merge queue | Not available: the rule was refused on a user-owned repository, and `strict` stays on. The 2026-09-17 measurements are in the issues. | The repository moves to an organization, or GitHub offers the queue on user-owned repositories. | [#2954](https://github.com/Rul1an/assay/issues/2954), [#2973](https://github.com/Rul1an/assay/issues/2973) |
| A documented-key to serialized-type mapper | Not built. The known wrong examples are corrected. | A documented key is found wrong in a way the existing guards miss. | [#3103](https://github.com/Rul1an/assay/issues/3103) |
| Authenticated profile and report schema identity | An unmet obligation. See the [evidence reference](cli/evidence.md). | A recipient shows its workflow, the bytes it receives, who it trusts to sign what, and the substitution it must reject, and why the signed artifact and a pinned release or schema revision cannot serve it. | [#3373](https://github.com/Rul1an/assay/issues/3373) |
