# chrono and humantime unmaintained notices, disposition 2026-11-17

**Open. Expires 2027-02-17.**

RustSec published two informational notices on 2026-11-17: RUSTSEC-2026-0543 (`chrono`)
and RUSTSEC-2026-0542 (`humantime`). Both say the crate is unmaintained and that every
version is affected. Neither describes a vulnerability and neither has a patched version.
Assay keeps `chrono` 0.4.45 and `humantime` 2.4.0 and ignores the two IDs, by ID and
until a date. This record says why, what would end the exception, and what it does not
establish.

## What the notices say

| ID | Crate | Kind | Patched | Advisory date | Published |
| --- | --- | --- | --- | --- | --- |
| RUSTSEC-2026-0543 | `chrono` | `informational = "unmaintained"` | none | 2026-09-14 | 2026-11-17 |
| RUSTSEC-2026-0542 | `humantime` | `informational = "unmaintained"` | none | 2026-09-14 | 2026-11-17 |

The advisory date is the day the notice was written. It was held back until `jiff` 1.0
was released (2026-11-15) and merged two days later, in
[rustsec/advisory-db#3214](https://github.com/rustsec/advisory-db/pull/3214). Both link
[chronotope/chrono#1768](https://github.com/chronotope/chrono/issues/1768), where the
maintainer announced in January 2026 that maintenance of `chrono` would wind down, and
said on 2026-09-15 that `chrono`, `chrono-tz` and `humantime` are "unlikely to merge any
more changes". The `chrono` notice names `jiff` and `time` as alternatives. The
`humantime` notice names none.

One sentence in the `humantime` notice does not match the artifact: it says `humantime`
"wraps chrono". `humantime` 2.4.0 has no dependencies (`cargo tree -p humantime@2.4.0`
prints the crate alone). The notice still stands on the maintainer's own statement that
the crate will see no further changes.

## Exposure in this repository

Measured on `ce68bfe570e01ae2435dcc8f36ac43617abbb4d9` with `cargo tree --locked -i`.

- `chrono` 0.4.45 is a direct dependency of ten workspace crates: `assay-cli`,
  `assay-common` (optional, behind `std`), `assay-core`, `assay-evidence`,
  `assay-registry`, `assay-sim`, and the unpublished `assay-adapter-a2a`,
  `assay-adapter-acp`, `assay-adapter-ucp` and `gateway-evidence-replay`.
- `humantime` 2.4.0 is a direct dependency of `assay-cli` only, for one command-line
  argument type (`assay monitor`'s duration).
- `object_store` 0.14.2, a dependency of `assay-evidence`, requires both crates and
  offers no feature to drop them. 0.14.2 is the newest release. Removing every use in
  Assay's own code would therefore leave both crates in `Cargo.lock` and both notices
  firing.
- `fuzz/Cargo.lock` carries the same two versions, `chrono` through `assay-common`,
  `assay-core`, `assay-evidence` and `object_store`, `humantime` through `object_store`.
- `chrono::DateTime<Utc>` is part of the public API of four published crates:
  `assay-evidence`, `assay-core`, `assay-registry` and `assay-common`. Replacing `chrono`
  there changes public types, which is a major version for each of them.

`chrono-tz` and `chrono-tz-build`, which received notices in the same batch, are not in
either lockfile.

## Risk

There is no known defect in either version. The risk is about the future: a defect found
in `chrono` or `humantime` from now on is unlikely to be fixed upstream. For Assay the
part that matters is that `chrono` parses timestamps from input Assay treats as hostile
(evidence bundles, registry responses, mandates). A parsing defect there would have to be
handled by replacing the crate, not by upgrading it. That is a reason to plan the
replacement. It is not a reason to call the current tree vulnerable.

## Enforcement and expiry

Three scanners read these lockfiles and they treat an unmaintained notice differently:

- `cargo-deny` 0.20.2 reports it as an error. `deny.toml` ignores the two IDs, each with
  a reason. `cargo-deny` has no expiry on an ignore.
- The OSV scanner (required `OSV Cargo.lock` job) fails on any finding. `osv-scanner.toml`
  and `fuzz/osv-scanner.toml` ignore the two IDs with `ignoreUntil = 2027-02-17`. The
  second file exists because OSV applies an `osv-scanner.toml` only to files in its own
  directory. When the date passes, the finding returns and the required job goes red.
  That is the expiry for the `deny.toml` entries too: they are reviewed on that day.
- `cargo-audit` reports it as a warning and exits 0. `.cargo/audit.toml` stays empty on
  purpose, so every audit run keeps printing both notices.

The ignores are by ID. A vulnerability reported against either crate gets a new ID and
fails all three scanners without anyone touching this exception.

Two rules keep the three files one exception, and `scripts/ci/check-osv-cargo-lock-gate.py`
fails when either is broken. An ID in both `osv-scanner.toml` files has the same
`ignoreUntil` in both. Every advisory `deny.toml` ignores has a dated entry in the root
`osv-scanner.toml`, so removing the OSV entries while leaving the `deny.toml` ones is
refused instead of leaving an ignore that never ends.

## What ends the exception

Remove an ID from `deny.toml` and both `osv-scanner.toml` files when its crate is no
longer in `Cargo.lock` and `fuzz/Cargo.lock`. That needs both of the following:

1. An `object_store` release that no longer requires the crate.
2. Assay's own use replaced. For `humantime` that is one argument type in `assay-cli`.
   For `chrono` it is a migration across ten crates with public types in four published
   ones, so it belongs with the next major version.

Review before the date, without waiting for it, if any of these happens:

- a vulnerability advisory is published against `chrono` or `humantime`;
- `object_store` releases a version without them;
- someone with standing takes over `chrono` and the notice is withdrawn, as happened to
  the earlier `humantime` notice RUSTSEC-2025-0014 in March 2025.

On 2027-02-17, if neither crate can leave the tree yet, extending the date needs a fresh
look at the three points above and a new entry in this record. It is not a date to move
forward by habit.

## What this does not establish

It does not establish that `chrono` 0.4.45 or `humantime` 2.4.0 are free of defects, that
Assay's use of them is unreachable by hostile input, or that upstream endorses this
disposition. It does not cover a future advisory against either crate.

## Reproduction

On `ce68bfe570e01ae2435dcc8f36ac43617abbb4d9`, with cargo-deny 0.20.2 and the advisory
database at or after rustsec/advisory-db `233155b402`:

- `cargo-deny check advisories bans licenses sources` without the two ignores ends with
  `advisories FAILED` and two `error[unmaintained]` findings, exit 1.
- With the two ignores it ends with `advisories ok, bans ok, licenses ok, sources ok`,
  exit 0, and prints no `advisory-not-detected` warning, so both ignores match a crate in
  the tree.
- `bash scripts/ci/cargo-audit-with-isolated-db.sh` exits 0 either way and lists both
  notices as warnings.
