# Codex Host Package Proof

The `assay.codex-host-proof.v6` driver requires retained Assay package evidence
before a `host-observation` run. This is not an installation recipe or release
acceptance by itself. Synthetic fixtures remain synthetic, even if their package
checks pass.

## Supported Route

This slice verifies the **crates.io source package** for `assay-mcp-server`.
The declared reference must be `assay-mcp-server@<exact-version>`. A local build
or host-bundled Assay does not qualify. A GitHub release is an admissible route
in principle, but this verifier does not yet verify that archive format: it
refuses rather than substitutes a source-package check.

Supply three explicit files; the driver never searches credentials or profiles:

- The retained `.crate` package, at most 4 MiB.
- The exact release's JSON row from the official
  [Cargo sparse index](https://index.crates.io/as/sa/assay-mcp-server), at most
  256 KiB. It must name the same crate/version, be non-yanked, and its `cksum`
  must equal the SHA-256 of the actual package bytes.
- The installation prefix's `.crates2.json`, at most 4 MiB. Its selected entry
  must name that registry package, an exact version requirement, the
  `assay-mcp-server` binary, and a release profile. The selected PATH binary
  must be in that prefix's `bin` directory. Only the selected package identity,
  version requirement, binary name, profile, target and rustc fields are retained,
  not other installed packages or the original metadata file. Cargo's verbose
  `rustc` value may contain LF or CRLF line endings within a 2048-character bound;
  its lines are printable ASCII. Versions and install references remain single-line.

An operator must independently check acquisition of the index row and package.
The offline validator does **not** fetch the index or authenticate copied index
bytes. `indexSource` names the expected official source, not a verified network
receipt. Cargo documents the checksum in its
[registry index contract](https://doc.rust-lang.org/cargo/reference/registry-index.html).

## Execution And Retention

For an otherwise prepared, authorized host journey, add:

```sh
--install-source assayMcp crates-io "assay-mcp-server@$VERSION" \
--assay-package "$PACKAGE_FILE" \
--assay-index-row "$INDEX_ROW_FILE" \
--assay-cargo-metadata "$INSTALL_PREFIX/.crates2.json"
```

All three file arguments must be absolute paths. Existing Codex and code-mode
host install-source declarations remain required. There is no new vendor-bundle
checksum obligation and no authentication change. A tool journey still requires
its explicit model-turn authorization.

The producer copies fixed-name binary snapshots, retains the package and row,
projects the selected Cargo metadata, and verifies them **before either version
probe**. It recomputes the retained package check before starting the host.
Missing or invalid evidence stops execution; acquisition failure can leave an
incomplete directory, which is not a valid proof. Use a fresh proof root for a
new attempt, not a repaired historical record.

Successful acquisition retains `assay-package.crate`, `assay-registry-row.json`
and `assay-install.json`. The manifest's `packageVerification` binds their hashes
and the binary snapshot hash. The offline consumer reads those files again and
recomputes the comparison; two matching digest strings do not suffice.

Every host journey requires both `installRouteAdmissible` and
`installPackageVerified` to pass. Missing, unavailable or failed cells do not
pass. No new `not_applicable` status or aggregate exception is introduced.

## Claim Boundary

The package-to-binary association is **Cargo installation metadata**, not a
cryptographic proof of compilation from those bytes. It proves neither a
reproducible build nor an uncompromised build environment. An internally
consistent record does not authenticate origin, authorship or execution order;
before-execution ordering is enforced by the reviewed producer, not proved by a
timestamp. The complete record still needs independent acquisition and host-run
review before it can support a launch claim.

Previously retained v4/v5 packs remain immutable and use their pinned verifiers.
The v6 validator rejects those schemas; do not retrofit new checks or fields into
old packs. A newly recomputed package checksum says nothing about whether that
check ran before an earlier host execution.
